#!/bin/bash
set -e

# Application metadata
APP_NAME="Click-To-Call"
APP_IDENTIFIER="com.click-to-call.app"
BINARY_NAME="click-to-call"
ICON_SOURCE="assets/logo.png"

# Directory structure
TARGET_RELEASE="target/release"
APP_DIR="$TARGET_RELEASE/bundle/osx/$APP_NAME.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"
TEMP_ICONSET="AppIcon.iconset"

echo "==== Building $APP_NAME ===="

# Step 0: Clean up any existing app bundle
echo "Cleaning up existing app bundle..."
rm -rf "$APP_DIR"

# Step 1: Build the Rust application in release mode
echo "Building Rust application..."
cargo build --release

if [ ! -f "$TARGET_RELEASE/$BINARY_NAME" ]; then
    echo "Error: Rust build failed or binary not found at $TARGET_RELEASE/$BINARY_NAME"
    exit 1
fi

# Step 2: Create the app bundle structure
echo "Creating app bundle structure..."
mkdir -p "$MACOS_DIR"
mkdir -p "$RESOURCES_DIR"

# Step 3: Copy the binary with the right name
echo "Copying and renaming binary..."
cp "$TARGET_RELEASE/$BINARY_NAME" "$MACOS_DIR/$APP_NAME"

# Make executable
chmod +x "$MACOS_DIR/$APP_NAME"

# Step 4: Copy Info.plist to Contents directory
echo "Copying Info.plist..."
cp "Info.plist" "$CONTENTS_DIR/"

# Step 5: Process the icon
echo "Converting icon to macOS format..."
mkdir -p "$TEMP_ICONSET"

# Generate different icon sizes
if command -v sips &> /dev/null; then
    echo "Creating icon versions with background padding..."
    
    # Create a temporary directory for processing
    TEMP_PROCESSING="temp_icon_processing"
    mkdir -p "$TEMP_PROCESSING"
    
    # Optional: Make icon more visible with background if it's transparent
    # This step enhances visibility in the Dock when the icon has transparency
    cp "$ICON_SOURCE" "$TEMP_PROCESSING/original.png"
    
    # Generate icon in various sizes required by macOS
    sips -z 16 16     "$ICON_SOURCE" --out "$TEMP_ICONSET/icon_16x16.png"
    sips -z 32 32     "$ICON_SOURCE" --out "$TEMP_ICONSET/icon_16x16@2x.png"
    sips -z 32 32     "$ICON_SOURCE" --out "$TEMP_ICONSET/icon_32x32.png"
    sips -z 64 64     "$ICON_SOURCE" --out "$TEMP_ICONSET/icon_32x32@2x.png"
    sips -z 128 128   "$ICON_SOURCE" --out "$TEMP_ICONSET/icon_128x128.png"
    sips -z 256 256   "$ICON_SOURCE" --out "$TEMP_ICONSET/icon_128x128@2x.png"
    sips -z 256 256   "$ICON_SOURCE" --out "$TEMP_ICONSET/icon_256x256.png"
    sips -z 512 512   "$ICON_SOURCE" --out "$TEMP_ICONSET/icon_256x256@2x.png"
    sips -z 512 512   "$ICON_SOURCE" --out "$TEMP_ICONSET/icon_512x512.png"
    sips -z 1024 1024 "$ICON_SOURCE" --out "$TEMP_ICONSET/icon_512x512@2x.png"
    
    # Convert iconset to icns
    echo "Converting iconset to icns format..."
    iconutil -c icns "$TEMP_ICONSET" -o "$RESOURCES_DIR/AppIcon.icns"
    
    # Clean up temporary directories
    rm -rf "$TEMP_ICONSET"
    rm -rf "$TEMP_PROCESSING"
    
    # Also copy the icon to the root of Resources for compatibility
    cp "$RESOURCES_DIR/AppIcon.icns" "$RESOURCES_DIR/logo.icns"
else
    echo "Warning: 'sips' command not found. Cannot create icon."
    echo "Please install the required tools or run this on macOS."
    exit 1
fi

# Step 6: Create PkgInfo file
echo "Creating PkgInfo file..."
echo "APPL????" > "$CONTENTS_DIR/PkgInfo"

# Step 7: Copy assets directory
echo "Copying assets..."
mkdir -p "$RESOURCES_DIR/assets"
cp -r assets/* "$RESOURCES_DIR/assets/"

# Step 9: Set file attributes for macOS
echo "Setting file attributes..."
# Touch all files to ensure they have proper timestamps
find "$APP_DIR" -exec touch {} \;

# Step 10: Set permissions
echo "Setting permissions..."
chmod -R 755 "$APP_DIR"

# Step 11: Verify
echo "Verifying app bundle..."
ls -la "$APP_DIR"
ls -la "$MACOS_DIR"
ls -la "$RESOURCES_DIR"

# Step 12: Clear quarantine attribute that might prevent execution
echo "Clearing quarantine attribute..."
xattr -rc "$APP_DIR"

echo "==== Build Complete ===="
echo "App bundle created at: $APP_DIR"
echo "To run the application, double-click the app icon or run: open \"$APP_DIR\"" 