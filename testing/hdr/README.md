# HDR Support in gst-wayland-display

## Overview

This document describes the HDR (High Dynamic Range) implementation in the gst-wayland-display project. The implementation follows the Wayland color management protocol and integrates with GStreamer's HDR capabilities.

## Architecture

### Components

1. **HDR Protocol Layer** (`wayland-display-core/src/wayland/protocols/hdr.rs`)
   - Implements HDR metadata structures
   - Handles EOTF (Electro-Optical Transfer Function) support
   - Manages color space information

2. **GStreamer Integration** (`gst-plugin-wayland-display/src/waylandsrc/imp.rs`)
   - Handles HDR metadata in caps negotiation
   - Supports HDR video formats
   - Manages HDR metadata passing

3. **Wayland Display Core** (`wayland-display-core/src/lib.rs`)
   - Manages HDR state
   - Handles metadata passing through command channel
   - Controls display output configuration

### Implementation Details

#### HDR Metadata Structure
```rust
pub struct HdrMetadata {
    pub eotf: Eotf,
    pub mastering_display_info: Option<MasteringDisplayInfo>,
    pub content_light_level: Option<ContentLightLevel>,
}
```

The implementation supports:
- HDR10 (PQ/ST2084)
- HLG (Hybrid Log-Gamma)
- SDR fallback

#### Color Space Support
- BT.2020 color space for HDR
- RGB10A2 format for HDR content
- Proper color space transformation

## Testing

### Prerequisites

1. HDR-capable display
2. HDR test content
3. GStreamer with HDR support
4. Wayland compositor with HDR support

### Test Scripts

Located in `testing/hdr/scripts/`:
- `test_hdr_pipeline.sh`: Basic HDR pipeline test
- `test_hdr_metadata.sh`: HDR metadata verification
- `test_hdr_formats.sh`: Format support verification

### Test Content

Sample HDR content is available in `testing/hdr/content/`:
- HDR10 test patterns
- HLG test patterns
- SDR reference content

### Running Tests

1. **Basic HDR Test**
```bash
./testing/hdr/scripts/test_hdr_pipeline.sh
```

2. **Metadata Verification**
```bash
./testing/hdr/scripts/test_hdr_metadata.sh
```

3. **Format Support Check**
```bash
./testing/hdr/scripts/test_hdr_formats.sh
```

## Implementation References

### Protocol Implementation
- Based on Weston's color management implementation
- Reference: [Weston Color Management](https://gitlab.freedesktop.org/weston/weston/-/merge_requests/1590)

### GStreamer Integration
- Follows GStreamer's HDR implementation
- Reference: [GStreamer HDR Support](https://gitlab.freedesktop.org/gstreamer/gstreamer/-/merge_requests/6830)

### Color Management
- Based on KWin's color management implementation
- Reference: [KWin Color Management](https://invent.kde.org/plasma/kwin/-/merge_requests/6126)

## Known Limitations

1. Currently supports HDR10 and HLG
2. Dolby Vision support pending
3. Limited color space transformation options
4. Requires HDR-capable display

## Future Improvements

1. Add Dolby Vision support
2. Implement dynamic metadata (HDR10+)
3. Add more color space transformation options
4. Improve tone mapping algorithms

## Debugging

### Enable Debug Logging
```bash
export GST_DEBUG=3
export GST_DEBUG_DUMP_DOT_DIR=./debug
```

### Common Issues

1. **HDR Not Working**
   - Check display HDR capabilities
   - Verify HDR metadata in pipeline
   - Check format support

2. **Color Space Issues**
   - Verify color space transformation
   - Check display color space support
   - Validate metadata values

## Contributing

When contributing to HDR support:
1. Follow existing code style
2. Add tests for new features
3. Update documentation
4. Test with various HDR content 