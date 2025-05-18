
# Click-To-Call - FusionPBX (Mac OS X)

## Using the app  
Install the latest release in /Applications  
Run the app, configure your settings (domain should be entered with out any protocol ie, "fusionpbx.example.com")  
Extension should be assigned to the user which key you are using.  
Enter key and extension.  
Open FaceTime app > Settings and change the default app to "Click-To-Call"  
Click on any `tel:` link (for Firefox you'll have to accept and approve, tick always allow / open)

Click-To-Call initiates a HTTP GET request to your FusionPBX server and places a call using the extension provided in settings. This is not a SIP phone, this utilizes your connected SIP extension and initiates the call, with option to auto-answer the initiated call. 


## Overview

Click-To-Call is a macOS utility that allows you to:
- Intercept `tel:` URLs and make calls through a web API
- Configure domain, extension, and authentication settings
- Place calls directly from the application
- Set auto-answer preferences

## Prerequisites

Before building, ensure you have the following installed:

- **macOS** - The build script is designed for macOS systems
- **Rust and Cargo** - Install via [rustup](https://rustup.rs/)
- **Xcode Command Line Tools** - Run `xcode-select --install` in Terminal
- **sips and iconutil** - These should be pre-installed on macOS
- **Git** - For cloning the repository (if needed)

## Directory Structure

Ensure your project has the following files:
- `build.sh` - The build script
- `Info.plist` - Application metadata
- `src/main.rs` - Application source code
- `assets/logo.png` - Application icon (1024×1024 recommended)

## Build Instructions

1. Clone or download the repository
2. Open Terminal and navigate to the project directory
3. Make the build script executable (if needed):
   ```
   chmod +x build.sh
   ```
4. Run the build script:
   ```
   ./build.sh
   ```
5. The script will:
   - Compile the Rust application in release mode
   - Create the macOS app bundle
   - Process the application icon
   - Set appropriate permissions

6. When completed, the built application will be located at:
   ```
   target/release/bundle/osx/Click-To-Call.app
   ```

## Running the Application

After building:

1. Double-click the built application in Finder
2. Or run from Terminal:
   ```
   open target/release/bundle/osx/Click-To-Call.app
   ```
3. Configure your domain, extension, and key settings
4. Click "Save Settings" to store your configuration

## URL Handling

The application registers as a handler for `tel:` URLs. After configuration, clicking telephone links in your browser will initiate calls through your configured system.

## Troubleshooting

- **"App is damaged and can't be opened"** - Run `xattr -rc target/release/bundle/osx/Click-To-Call.app` to remove quarantine attributes
- **Build fails with "command not found"** - Ensure Rust and Xcode CLI tools are properly installed
- **Icon doesn't appear** - Verify that `assets/logo.png` exists and is a valid PNG image
- **Application doesn't launch** - Check Terminal output for errors after running the build script

## Customization

To customize the application:
- Edit `Info.plist` to change application metadata
- Replace `assets/logo.png` with your own icon (1024×1024px recommended)
- Modify the `APP_NAME` and `APP_IDENTIFIER` variables in `build.sh` if needed
