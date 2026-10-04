//! Server side of `wlr-output-management-unstable-v1`: the one protocol a Wayland client can
//! use to ask the compositor for a different output mode (resolution + refresh rate).
//!
//! The compositor has exactly one output. It is described as one head, whose mode list is
//! whatever `wl_output` advertises -- the current mode, the mode ladder, and the display's
//! real mode set (`Command::OutputModes`, each entry with its own refresh). A client picks a
//! mode with `enable_head` + `set_mode` (or `set_custom_mode` matching an advertised one) and
//! `apply`s:
//!
//! * the configuration is answered `succeeded` and forwarded as `Command::ModeRequest` to the
//!   element, whose owner moves the physical display and re-negotiates the caps -- the
//!   compositor never changes its own mode here, so a request the owner refuses leaves
//!   everything exactly as it was;
//! * a mode that is not advertised, or a configuration that disables the only head, is
//!   `failed`; a configuration built against a stale serial is `cancelled`.
//!
//! Position, transform, scale and adaptive-sync are accepted and ignored: there is one head
//! at (0,0), composited at scale 1 (the UI scale is announced separately through
//! `wp_fractional_scale_v1`), and refresh is governed by the pipeline's pull.
//!
//! Every mode-set or current-mode change goes through [`OutputManagementState::publish`],
//! which re-describes the head to every bound manager and bumps the serial.

use std::sync::{Arc, Mutex};

use smithay::output::{Mode as OutputMode, Output};
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, WEnum,
    backend::GlobalId,
};
use wayland_protocols_wlr::output_management::v1::server::{
    zwlr_output_configuration_head_v1::{self, ZwlrOutputConfigurationHeadV1},
    zwlr_output_configuration_v1::{self, ZwlrOutputConfigurationV1},
    zwlr_output_head_v1::{self, AdaptiveSyncState, ZwlrOutputHeadV1},
    zwlr_output_manager_v1::{self, ZwlrOutputManagerV1},
    zwlr_output_mode_v1::{self, ZwlrOutputModeV1},
};

use crate::comp::{State, request_mode};

/// Highest protocol version advertised. v4 adds `adaptive_sync`, which is accepted and
/// ignored (the pipeline's pull rate governs refresh).
const VERSION: u32 = 4;

/// Two refresh rates within this many millihertz name the same mode: a client that read
/// `143.981 Hz` and sends `144000` back as a custom mode means the same thing.
const REFRESH_TOLERANCE_MHZ: i32 = 500;

/// Name of the one head, matching the `wl_output` name clients already see.
const HEAD_NAME: &str = "HEADLESS-1";

/// One bound `zwlr_output_manager_v1` and the objects it has been told about.
struct Binding {
    manager: ZwlrOutputManagerV1,
    head: Option<ZwlrOutputHeadV1>,
    modes: Vec<(OutputMode, ZwlrOutputModeV1)>,
    /// The current mode last announced to this binding, so an unchanged one is not re-sent.
    current: Option<OutputMode>,
}

pub struct OutputManagementState {
    _global: GlobalId,
    bindings: Vec<Binding>,
    serial: u32,
}

/// What a client has asked for in one `zwlr_output_configuration_v1`, shared between the
/// configuration object and its per-head configuration object.
#[derive(Default)]
struct ConfigRequest {
    serial: u32,
    head_enabled: bool,
    head_disabled: bool,
    mode: Option<OutputMode>,
    custom: Option<(i32, i32, i32)>,
    used: bool,
}

type ConfigData = Arc<Mutex<ConfigRequest>>;

impl OutputManagementState {
    pub fn new<D>(dh: &DisplayHandle) -> Self
    where
        D: GlobalDispatch<ZwlrOutputManagerV1, ()> + 'static,
    {
        let global = dh.create_global::<D, ZwlrOutputManagerV1, ()>(VERSION, ());
        Self {
            _global: global,
            bindings: Vec::new(),
            serial: 0,
        }
    }

    /// Whether any client currently holds a live `zwlr_output_manager_v1`. A guest that
    /// binds the manager asks for modes explicitly, so the `follow-client-size` fallback
    /// (`maybe_follow_client_size`) defers to it while this is true. Dead bindings are
    /// pruned on `stop` and when the resource is destroyed (including client disconnect);
    /// the `is_alive` filter is a belt-and-braces guard for the window in between.
    pub fn has_bound_managers(&self) -> bool {
        self.bindings.iter().any(|b| b.manager.is_alive())
    }

    /// The serial the next `create_configuration` must carry.
    pub fn serial(&self) -> u32 {
        self.serial
    }

    /// Re-describe `output` to every bound manager -- new modes, retired modes, the current
    /// mode -- and send `done` with a fresh serial. Called after every change to the
    /// advertised mode set or the current mode, and once per bind.
    pub fn publish(&mut self, dh: &DisplayHandle, output: &Output) {
        self.bindings.retain(|b| b.manager.is_alive());
        if self.bindings.is_empty() {
            return;
        }
        self.serial = self.serial.wrapping_add(1);
        let serial = self.serial;
        for binding in &mut self.bindings {
            describe(binding, dh, output);
            binding.manager.done(serial);
        }
    }

    fn bind(&mut self, dh: &DisplayHandle, manager: ZwlrOutputManagerV1, output: Option<&Output>) {
        let mut binding = Binding {
            manager,
            head: None,
            modes: Vec::new(),
            current: None,
        };
        if let Some(output) = output {
            describe(&mut binding, dh, output);
        }
        // A fresh binding always gets the current serial, even with no head yet: the spec
        // says `done` follows the initial head burst, and a configuration against this
        // serial is valid until the next change.
        binding.manager.done(self.serial);
        self.bindings.push(binding);
    }

    /// The advertised mode matching `(w, h, refresh)`: same size, nearest refresh within
    /// [`REFRESH_TOLERANCE_MHZ`]. A refresh of `0` ("any") takes the current mode's refresh
    /// when that size has one, else the highest.
    fn resolve(output: &Output, w: i32, h: i32, refresh: i32) -> Option<OutputMode> {
        let same_size: Vec<OutputMode> = output
            .modes()
            .into_iter()
            .filter(|m| m.size.w == w && m.size.h == h)
            .collect();
        if refresh <= 0 {
            if let Some(cur) = output.current_mode().filter(|c| same_size.contains(c)) {
                return Some(cur);
            }
            return same_size.into_iter().max_by_key(|m| m.refresh);
        }
        same_size
            .into_iter()
            .min_by_key(|m| (m.refresh - refresh).abs())
            .filter(|m| (m.refresh - refresh).abs() <= REFRESH_TOLERANCE_MHZ)
    }
}

/// Send `binding` everything about `output` it does not already know.
fn describe(binding: &mut Binding, dh: &DisplayHandle, output: &Output) {
    let Some(client) = binding.manager.client() else {
        return;
    };
    let version = binding.manager.version();
    let head = match &binding.head {
        Some(head) => head.clone(),
        None => {
            let Ok(head) = client.create_resource::<ZwlrOutputHeadV1, (), State>(dh, version, ())
            else {
                return;
            };
            binding.manager.head(&head);
            head.name(HEAD_NAME.into());
            head.description(format!(
                "{} {}",
                output.physical_properties().make,
                output.physical_properties().model
            ));
            let size = output.physical_properties().size;
            head.physical_size(size.w, size.h);
            if version >= 2 {
                head.make(output.physical_properties().make);
                head.model(output.physical_properties().model);
                head.serial_number(String::new());
            }
            head.enabled(1);
            head.position(0, 0);
            head.transform(
                smithay::reexports::wayland_server::protocol::wl_output::Transform::Normal,
            );
            head.scale(1.0);
            if version >= 4 {
                head.adaptive_sync(AdaptiveSyncState::Disabled);
            }
            binding.head = Some(head.clone());
            head
        }
    };

    // Modes: retire the ones no longer advertised, introduce the new ones.
    let advertised = output.modes();
    let preferred = output.preferred_mode();
    binding.modes.retain(|(mode, res)| {
        if advertised.contains(mode) {
            true
        } else {
            res.finished();
            false
        }
    });
    for mode in &advertised {
        if binding.modes.iter().any(|(m, _)| m == mode) {
            continue;
        }
        let Ok(res) =
            client.create_resource::<ZwlrOutputModeV1, OutputMode, State>(dh, version, *mode)
        else {
            continue;
        };
        head.mode(&res);
        res.size(mode.size.w, mode.size.h);
        res.refresh(mode.refresh);
        if Some(*mode) == preferred {
            res.preferred();
        }
        binding.modes.push((*mode, res));
    }

    // The current mode, only when it moved.
    let current = output.current_mode();
    if current != binding.current {
        if let Some((_, res)) = current.and_then(|c| binding.modes.iter().find(|(m, _)| *m == c)) {
            head.current_mode(res);
        }
        binding.current = current;
    }
}

impl GlobalDispatch<ZwlrOutputManagerV1, ()> for State {
    fn bind(
        state: &mut State,
        dh: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrOutputManagerV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, State>,
    ) {
        let manager = data_init.init(resource, ());
        let output = state.output.clone();
        state.output_mgmt.bind(dh, manager, output.as_ref());
    }
}

impl Dispatch<ZwlrOutputManagerV1, ()> for State {
    fn request(
        state: &mut State,
        _client: &Client,
        manager: &ZwlrOutputManagerV1,
        request: zwlr_output_manager_v1::Request,
        _data: &(),
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, State>,
    ) {
        match request {
            zwlr_output_manager_v1::Request::CreateConfiguration { id, serial } => {
                data_init.init(
                    id,
                    Arc::new(Mutex::new(ConfigRequest {
                        serial,
                        ..Default::default()
                    })),
                );
            }
            zwlr_output_manager_v1::Request::Stop => {
                manager.finished();
                state.output_mgmt.bindings.retain(|b| b.manager != *manager);
            }
            _ => {}
        }
    }

    /// The manager is gone -- `finished` was sent after `stop`, or the client disconnected
    /// without ever stopping. Drop its binding here rather than at the next `publish`, so
    /// [`OutputManagementState::has_bound_managers`] stops deferring the follow fallback the
    /// moment the guest that spoke the protocol is gone.
    fn destroyed(
        state: &mut State,
        _client: smithay::reexports::wayland_server::backend::ClientId,
        manager: &ZwlrOutputManagerV1,
        _data: &(),
    ) {
        state
            .output_mgmt
            .bindings
            .retain(|b| b.manager != *manager && b.manager.is_alive());
    }
}

impl Dispatch<ZwlrOutputHeadV1, ()> for State {
    fn request(
        _state: &mut State,
        _client: &Client,
        _head: &ZwlrOutputHeadV1,
        _request: zwlr_output_head_v1::Request,
        _data: &(),
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        // `release` is the only request.
    }
}

impl Dispatch<ZwlrOutputModeV1, OutputMode> for State {
    fn request(
        _state: &mut State,
        _client: &Client,
        _mode: &ZwlrOutputModeV1,
        _request: zwlr_output_mode_v1::Request,
        _data: &OutputMode,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        // `release` is the only request.
    }
}

impl Dispatch<ZwlrOutputConfigurationV1, ConfigData> for State {
    fn request(
        state: &mut State,
        _client: &Client,
        config: &ZwlrOutputConfigurationV1,
        request: zwlr_output_configuration_v1::Request,
        data: &ConfigData,
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, State>,
    ) {
        match request {
            zwlr_output_configuration_v1::Request::EnableHead { id, head: _ } => {
                {
                    let mut req = data.lock().unwrap();
                    if req.head_enabled || req.head_disabled {
                        config.post_error(
                            zwlr_output_configuration_v1::Error::AlreadyConfiguredHead,
                            "head already configured",
                        );
                        return;
                    }
                    req.head_enabled = true;
                }
                data_init.init(id, data.clone());
            }
            zwlr_output_configuration_v1::Request::DisableHead { head: _ } => {
                let mut req = data.lock().unwrap();
                if req.head_enabled || req.head_disabled {
                    config.post_error(
                        zwlr_output_configuration_v1::Error::AlreadyConfiguredHead,
                        "head already configured",
                    );
                    return;
                }
                req.head_disabled = true;
            }
            zwlr_output_configuration_v1::Request::Apply => resolve(state, config, data, true),
            zwlr_output_configuration_v1::Request::Test => resolve(state, config, data, false),
            zwlr_output_configuration_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

/// Answer an `apply` (`apply == true`) or `test`: `cancelled` on a stale serial, `failed`
/// when the configuration disables the only head or names a mode that is not advertised,
/// else `succeeded` -- and, on `apply`, the request goes to the element.
fn resolve(state: &mut State, config: &ZwlrOutputConfigurationV1, data: &ConfigData, apply: bool) {
    let mut req = data.lock().unwrap();
    if req.used {
        config.post_error(
            zwlr_output_configuration_v1::Error::AlreadyUsed,
            "configuration already applied or tested",
        );
        return;
    }
    req.used = true;
    if !req.head_enabled && !req.head_disabled {
        config.post_error(
            zwlr_output_configuration_v1::Error::UnconfiguredHead,
            "the head was neither enabled nor disabled",
        );
        return;
    }
    if req.serial != state.output_mgmt.serial() {
        config.cancelled();
        return;
    }
    if req.head_disabled {
        tracing::info!("wlr-output-management: refusing to disable the only output");
        config.failed();
        return;
    }
    let Some(output) = state.output.clone() else {
        config.failed();
        return;
    };
    let wanted = match (req.mode, req.custom) {
        (Some(mode), _) => Some((mode.size.w, mode.size.h, mode.refresh)),
        (None, Some(custom)) => Some(custom),
        (None, None) => None,
    };
    let Some((w, h, refresh)) = wanted else {
        // Enabled, no mode: nothing to change.
        config.succeeded();
        return;
    };
    let Some(mode) = OutputManagementState::resolve(&output, w, h, refresh) else {
        tracing::info!(
            w,
            h,
            refresh,
            "wlr-output-management: requested mode is not advertised"
        );
        config.failed();
        return;
    };
    config.succeeded();
    if apply && Some(mode) != output.current_mode() {
        tracing::info!(
            width = mode.size.w,
            height = mode.size.h,
            refresh_mhz = mode.refresh,
            "wlr-output-management: client applied a mode"
        );
        request_mode(state, mode);
    }
}

impl Dispatch<ZwlrOutputConfigurationHeadV1, ConfigData> for State {
    fn request(
        _state: &mut State,
        _client: &Client,
        head: &ZwlrOutputConfigurationHeadV1,
        request: zwlr_output_configuration_head_v1::Request,
        data: &ConfigData,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        let mut req = data.lock().unwrap();
        match request {
            zwlr_output_configuration_head_v1::Request::SetMode { mode } => {
                if req.mode.is_some() || req.custom.is_some() {
                    head.post_error(
                        zwlr_output_configuration_head_v1::Error::AlreadySet,
                        "mode already set",
                    );
                    return;
                }
                match mode.data::<OutputMode>() {
                    Some(mode) => req.mode = Some(*mode),
                    None => head.post_error(
                        zwlr_output_configuration_head_v1::Error::InvalidMode,
                        "mode does not belong to this head",
                    ),
                }
            }
            zwlr_output_configuration_head_v1::Request::SetCustomMode {
                width,
                height,
                refresh,
            } => {
                if req.mode.is_some() || req.custom.is_some() {
                    head.post_error(
                        zwlr_output_configuration_head_v1::Error::AlreadySet,
                        "mode already set",
                    );
                    return;
                }
                if width <= 0 || height <= 0 || refresh < 0 {
                    head.post_error(
                        zwlr_output_configuration_head_v1::Error::InvalidCustomMode,
                        "custom mode must have a positive size and a non-negative refresh",
                    );
                    return;
                }
                req.custom = Some((width, height, refresh));
            }
            // One head at the origin, composited at scale 1; these are accepted and ignored.
            zwlr_output_configuration_head_v1::Request::SetPosition { .. }
            | zwlr_output_configuration_head_v1::Request::SetTransform { .. } => {}
            zwlr_output_configuration_head_v1::Request::SetScale { scale } => {
                if scale <= 0.0 {
                    head.post_error(
                        zwlr_output_configuration_head_v1::Error::InvalidScale,
                        "scale must be positive",
                    );
                }
            }
            zwlr_output_configuration_head_v1::Request::SetAdaptiveSync {
                state: WEnum::Unknown(_),
            } => {
                head.post_error(
                    zwlr_output_configuration_head_v1::Error::InvalidAdaptiveSyncState,
                    "unknown adaptive sync state",
                );
            }
            zwlr_output_configuration_head_v1::Request::SetAdaptiveSync { .. } => {}
            _ => {}
        }
    }
}
