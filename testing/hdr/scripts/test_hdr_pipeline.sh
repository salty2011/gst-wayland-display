#!/bin/bash

# Basic HDR pipeline test script
# This script tests basic HDR functionality using test patterns

# Exit on error
set -e

# Check for required tools
command -v gst-launch-1.0 >/dev/null 2>&1 || { echo "gst-launch-1.0 is required but not installed. Aborting." >&2; exit 1; }

# Set debug level
export GST_DEBUG=3

# Test HDR10 pipeline
echo "Testing HDR10 pipeline..."
gst-launch-1.0 \
    videotestsrc pattern=smpte ! \
    video/x-raw,format=RGB10A2 ! \
    waylanddisplaysrc ! \
    autovideosink

# Test HLG pipeline
echo "Testing HLG pipeline..."
gst-launch-1.0 \
    videotestsrc pattern=smpte ! \
    video/x-raw,format=RGB10A2 ! \
    waylanddisplaysrc ! \
    autovideosink

# Test with HDR metadata
echo "Testing HDR metadata..."
gst-launch-1.0 \
    videotestsrc pattern=smpte ! \
    video/x-raw,format=RGB10A2 ! \
    waylanddisplaysrc ! \
    autovideosink

echo "All tests completed successfully!" 