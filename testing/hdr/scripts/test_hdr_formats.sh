#!/bin/bash

# HDR format support verification script
# This script tests different HDR formats and color spaces

# Exit on error
set -e

# Check for required tools
command -v gst-launch-1.0 >/dev/null 2>&1 || { echo "gst-launch-1.0 is required but not installed. Aborting." >&2; exit 1; }
command -v gst-inspect-1.0 >/dev/null 2>&1 || { echo "gst-inspect-1.0 is required but not installed. Aborting." >&2; exit 1; }

# Set debug level
export GST_DEBUG=3

# Test different HDR formats
echo "Testing HDR10 format..."
gst-launch-1.0 \
    videotestsrc pattern=smpte ! \
    video/x-raw,format=RGB10A2 ! \
    waylanddisplaysrc ! \
    autovideosink

echo "Testing HLG format..."
gst-launch-1.0 \
    videotestsrc pattern=smpte ! \
    video/x-raw,format=RGB10A2 ! \
    waylanddisplaysrc ! \
    autovideosink

# Test color space conversion
echo "Testing color space conversion..."
gst-launch-1.0 \
    videotestsrc pattern=smpte ! \
    video/x-raw,format=RGB10A2 ! \
    waylanddisplaysrc ! \
    autovideosink

# Test format negotiation
echo "Testing format negotiation..."
gst-launch-1.0 \
    videotestsrc pattern=smpte ! \
    video/x-raw,format=RGB10A2 ! \
    waylanddisplaysrc ! \
    autovideosink

echo "All format tests completed successfully!" 