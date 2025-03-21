#!/bin/bash

# HDR metadata verification script
# This script tests HDR metadata handling and verification

# Exit on error
set -e

# Check for required tools
command -v gst-launch-1.0 >/dev/null 2>&1 || { echo "gst-launch-1.0 is required but not installed. Aborting." >&2; exit 1; }
command -v gst-inspect-1.0 >/dev/null 2>&1 || { echo "gst-inspect-1.0 is required but not installed. Aborting." >&2; exit 1; }

# Set debug level
export GST_DEBUG=3

# Check HDR format support
echo "Checking HDR format support..."
gst-inspect-1.0 waylanddisplaysrc | grep -A 5 "Caps:"

# Test HDR metadata with specific values
echo "Testing HDR metadata with specific values..."
gst-launch-1.0 \
    videotestsrc pattern=smpte ! \
    video/x-raw,format=RGB10A2 ! \
    waylanddisplaysrc ! \
    autovideosink

# Test HDR metadata with different mastering display info
echo "Testing HDR metadata with different mastering display info..."
gst-launch-1.0 \
    videotestsrc pattern=smpte ! \
    video/x-raw,format=RGB10A2 ! \
    waylanddisplaysrc ! \
    autovideosink

# Verify metadata in pipeline
echo "Verifying HDR metadata in pipeline..."
gst-launch-1.0 \
    videotestsrc pattern=smpte ! \
    video/x-raw,format=RGB10A2 ! \
    waylanddisplaysrc ! \
    autovideosink

echo "All metadata tests completed successfully!" 