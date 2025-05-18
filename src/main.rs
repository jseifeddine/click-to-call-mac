use druid::widget::{Button, Checkbox, Flex, Label, TextBox};
use druid::{AppLauncher, Data, Env, Lens, LocalizedString, PlatformError, Widget, WidgetExt, WindowDesc};
use druid::AppDelegate;
use druid::Command;
use druid::DelegateCtx;
use druid::Selector;
use druid::Target;
use druid::Handled;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::thread;
use std::env;
use std::ffi::CString;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::Duration;

// Define a custom command to initiate a call
const MAKE_CALL: Selector = Selector::new("app.make-call");
// Command to run when app is fully initialized
const APP_INITIALIZED: Selector = Selector::new("app.initialized");
// Command to process external tel: URL
const PROCESS_TEL_URL: Selector<String> = Selector::new("app.process-tel-url");

// Socket path for inter-process communication
fn get_socket_path() -> PathBuf {
    dirs::runtime_dir()
        .unwrap_or_else(|| std::env::temp_dir())
        .join("click-to-call.sock")
}

// Application data model
#[derive(Clone, Data, Default, Serialize, Deserialize)]
struct AppState {
    domain: String,
    extension: String,
    key: String,
    auto_answer: bool,
    #[serde(skip)]
    phone_number: String,
    #[serde(skip)]
    status_message: String,
}

struct DomainLens;
struct ExtensionLens;
struct KeyLens;
struct AutoAnswerLens;
struct PhoneNumberLens;
struct StatusMessageLens;

impl Lens<AppState, String> for DomainLens {
    fn with<V, F: FnOnce(&String) -> V>(&self, data: &AppState, f: F) -> V {
        f(&data.domain)
    }

    fn with_mut<V, F: FnOnce(&mut String) -> V>(&self, data: &mut AppState, f: F) -> V {
        f(&mut data.domain)
    }
}

impl Lens<AppState, String> for ExtensionLens {
    fn with<V, F: FnOnce(&String) -> V>(&self, data: &AppState, f: F) -> V {
        f(&data.extension)
    }

    fn with_mut<V, F: FnOnce(&mut String) -> V>(&self, data: &mut AppState, f: F) -> V {
        f(&mut data.extension)
    }
}

impl Lens<AppState, String> for KeyLens {
    fn with<V, F: FnOnce(&String) -> V>(&self, data: &AppState, f: F) -> V {
        f(&data.key)
    }

    fn with_mut<V, F: FnOnce(&mut String) -> V>(&self, data: &mut AppState, f: F) -> V {
        f(&mut data.key)
    }
}

impl Lens<AppState, bool> for AutoAnswerLens {
    fn with<V, F: FnOnce(&bool) -> V>(&self, data: &AppState, f: F) -> V {
        f(&data.auto_answer)
    }

    fn with_mut<V, F: FnOnce(&mut bool) -> V>(&self, data: &mut AppState, f: F) -> V {
        f(&mut data.auto_answer)
    }
}

impl Lens<AppState, String> for PhoneNumberLens {
    fn with<V, F: FnOnce(&String) -> V>(&self, data: &AppState, f: F) -> V {
        f(&data.phone_number)
    }

    fn with_mut<V, F: FnOnce(&mut String) -> V>(&self, data: &mut AppState, f: F) -> V {
        f(&mut data.phone_number)
    }
}

impl Lens<AppState, String> for StatusMessageLens {
    fn with<V, F: FnOnce(&String) -> V>(&self, data: &AppState, f: F) -> V {
        f(&data.status_message)
    }

    fn with_mut<V, F: FnOnce(&mut String) -> V>(&self, data: &mut AppState, f: F) -> V {
        f(&mut data.status_message)
    }
}

// App delegate to handle custom commands
struct Delegate {
    auto_call: bool,
    phone_number: String,
    is_primary: bool,
}

impl AppDelegate<AppState> for Delegate {
    fn command(
        &mut self,
        ctx: &mut DelegateCtx,
        _target: Target,
        cmd: &Command,
        data: &mut AppState,
        _env: &Env,
    ) -> Handled {
        if cmd.is(MAKE_CALL) {
            // Make sure we have the necessary data
            if data.domain.is_empty() || data.extension.is_empty() || data.phone_number.is_empty() {
                data.status_message = "Error: Missing domain, extension or phone number".to_string();
                return Handled::Yes;
            }
            
            // Clone the data we need for the HTTP request
            let domain = data.domain.clone();
            let extension = data.extension.clone();
            let key = data.key.clone();
            let phone_number = data.phone_number.clone();
            let auto_answer = data.auto_answer;
            
            // Update UI immediately
            data.status_message = format!("Initiating call to {}...", phone_number);
            
            // Create event sink to update UI after HTTP request
            let event_sink = ctx.get_external_handle();
            
            // Spawn a thread for the HTTP request
            thread::spawn(move || {
                // Construct the URL
                let auto_answer_str = if auto_answer { "true" } else { "false" };
                
                // Make sure domain doesn't already have https://
                let domain_with_scheme = if domain.starts_with("http://") || domain.starts_with("https://") {
                    domain
                } else {
                    format!("https://{}", domain)
                };
                
                // Construct the URL based on the example
                let url_str = format!(
                    "{}/app/click_to_call/click_to_call.php?src_cid_name={}&src_cid_number={}&dest_cid_name={}&dest_cid_number={}&src={}&dest={}&auto_answer={}&rec=&ringback=us-ring&key={}",
                    domain_with_scheme, phone_number, phone_number, phone_number, phone_number, extension, phone_number, auto_answer_str, key
                );
                
                // Make the HTTP request
                let result = match Client::new().get(url_str).send() {
                    Ok(_) => format!("Call initialized to {}", phone_number),
                    Err(e) => format!("Error: {}", e),
                };
                
                // Update the UI with the result
                let result_clone = result.clone();
                event_sink.add_idle_callback(move |data: &mut AppState| {
                    data.status_message = result_clone;
                });
            });
            return Handled::Yes;
        } else if cmd.is(APP_INITIALIZED) {
            // App is now fully initialized, check if we should auto-call
            if self.auto_call && !self.phone_number.is_empty() && !data.domain.is_empty() && !data.extension.is_empty() {
                // Set the phone number in the app state
                data.phone_number = self.phone_number.clone();
                data.status_message = format!("Received tel: link. Calling: {}", self.phone_number);
                
                // Immediately initiate the call
                ctx.submit_command(MAKE_CALL);
                self.auto_call = false; // Prevent repeated calls
            }
            
            // If this is the primary instance, start the socket listener
            if self.is_primary {
                let event_sink = ctx.get_external_handle();
                
                // Start the socket listener in a separate thread
                thread::spawn(move || {
                    let socket_path = get_socket_path();
                    
                    // Try to create the listener
                    if let Ok(listener) = UnixListener::bind(&socket_path) {
                        listener.set_nonblocking(true).ok();
                        
                        loop {
                            match listener.accept() {
                                Ok((mut stream, _)) => {
                                    let mut buffer = [0; 1024];
                                    if let Ok(size) = stream.read(&mut buffer) {
                                        if size > 0 {
                                            if let Ok(message) = String::from_utf8(buffer[0..size].to_vec()) {
                                                if message.starts_with("tel:") {
                                                    // Send the phone number to the main thread
                                                    event_sink.submit_command(
                                                        PROCESS_TEL_URL, 
                                                        message, 
                                                        Target::Auto
                                                    ).ok();
                                                }
                                            }
                                        }
                                    }
                                }
                                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                    // No connection available, just sleep a bit
                                    thread::sleep(Duration::from_millis(100));
                                }
                                Err(_) => {
                                    // Some other error occurred
                                    break;
                                }
                            }
                        }
                    }
                });
            }
            
            return Handled::Yes;
        } else if let Some(url) = cmd.get(PROCESS_TEL_URL) {
            if url.starts_with("tel:") {
                // Extract phone number
                let raw_number = url.split_at(4).1.to_string();
                println!("Processing tel: URL with number: {}", raw_number);
                
                // Clean phone number but keep the plus sign
                let clean_number = raw_number
                    .replace("-", "")
                    .replace(" ", "")
                    .replace("(", "")
                    .replace(")", "");
                
                // Process the phone number if the domain and extension are configured
                if !data.domain.is_empty() && !data.extension.is_empty() {
                    data.phone_number = clean_number;
                    data.status_message = format!("Processing tel: URL: {}", raw_number);
                    
                    // Bring the window to front
                    #[cfg(target_os = "macos")]
                    {
                        use objc::{msg_send, sel, sel_impl};
                        use objc::runtime::{Class, Object};
                        
                        unsafe {
                            let cls = Class::get("NSApplication").unwrap();
                            let app: *const Object = msg_send![cls, sharedApplication];
                            let _: () = msg_send![app, activateIgnoringOtherApps:true];
                        }
                    }
                    
                    // Initiate the call
                    ctx.submit_command(MAKE_CALL);
                }
            }
            return Handled::Yes;
        }
        Handled::No
    }

    fn window_added(
        &mut self,
        id: druid::WindowId,
        _handle: druid::WindowHandle,
        _data: &mut AppState,
        _env: &Env,
        ctx: &mut DelegateCtx,
    ) {
        // Window is created, but might not be fully ready
        // Schedule APP_INITIALIZED command with a small delay
        let handle = ctx.get_external_handle();
        thread::spawn(move || {
            thread::sleep(std::time::Duration::from_millis(1000));
            handle.submit_command(APP_INITIALIZED, (), Target::Window(id)).ok();
        });
    }
}

fn main() -> Result<(), PlatformError> {
    // Check if the app is already running
    let socket_path = get_socket_path();
    let is_primary = !try_connect_to_primary(&socket_path);
    
    // Register apple event handler for MacOS URL scheme (only for primary instance)
    #[cfg(target_os = "macos")]
    if is_primary {
        use objc::{msg_send, sel, sel_impl};
        use objc::runtime::{Class, Object, Sel};
        
        unsafe {
            extern "C" fn handle_url_event(_this: &Object, _: Sel, event: *const Object, _: *const Object) {
                // Apple Event constants
                const KEY_DIRECT_OBJECT: u32 = 0x2D2D2D2D; // ---- in UTF-8 (keyDirectObject)
                
                unsafe {
                    let desc: *const Object = msg_send![event, paramDescriptorForKeyword: KEY_DIRECT_OBJECT];
                    let url_str: *const Object = msg_send![desc, stringValue];
                    let ns_string: *const Object = msg_send![url_str, UTF8String];
                    let c_str = std::ffi::CStr::from_ptr(ns_string as *const i8);
                    
                    if let Ok(url) = c_str.to_str() {
                        println!("Received URL: {}", url);
                        if url.starts_with("tel:") {
                            // Process within the current instance instead of launching a new one
                            if let Ok(mut stream) = UnixStream::connect(get_socket_path()) {
                                let _ = stream.write_all(url.as_bytes());
                            }
                        }
                    }
                }
            }
            
            let cls = Class::get("NSAppleEventManager").unwrap();
            let manager: *const Object = msg_send![cls, sharedAppleEventManager];
            
            // Register handler for URL events
            let app_delegate_class = Class::get("NSObject").unwrap();
            let sel_handle_url = sel!(handleURLEvent:withReplyEvent:);
            
            // Apple Event class and ID for URL handling
            // 'GURL' in UTF-8 (Generic URL)
            const GURL_EVENT_CLASS: u32 = 0x4755524C; // 'GURL'
            const GURL_EVENT_ID: u32 = 0x4755524C;    // 'GURL'
            
            // Create C string for method signature
            let types = CString::new("v@:@@").unwrap();
            
            class_addMethod(
                app_delegate_class,
                sel_handle_url,
                handle_url_event as extern "C" fn(&Object, Sel, *const Object, *const Object),
                types.as_ptr()
            );
            
            let delegate: *const Object = msg_send![app_delegate_class, new];
            let _: () = msg_send![manager, 
                          setEventHandler:delegate 
                          andSelector:sel_handle_url 
                          forEventClass:GURL_EVENT_CLASS 
                          andEventID:GURL_EVENT_ID];
        }
    }

    // Check for tel: URL in app arguments
    let initial_state = load_preferences();
    let mut auto_call = false;
    let mut phone_number = String::new();
    
    // Print all args for debugging
    println!("Received arguments: {:?}", env::args().collect::<Vec<_>>());
    
    // On macOS, the URL is passed through the process arguments
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        // Look for tel: URL in all arguments
        for arg in &args[1..] {
            println!("Checking arg: {}", arg);
            
            // Check for tel: prefix (case insensitive)
            let arg_lower = arg.to_lowercase();
            if arg_lower.starts_with("tel:") {
                // If this is not the primary instance, try to send the URL to the primary instance
                if !is_primary {
                    if let Ok(mut stream) = UnixStream::connect(&socket_path) {
                        if stream.write_all(arg.as_bytes()).is_ok() {
                            // Successfully sent to primary instance, exit this one
                            println!("Sent URL to primary instance and exiting");
                            return Ok(());
                        }
                    }
                }
                
                // Extract phone number
                let raw_number = arg.split_at(4).1.to_string();
                println!("Found tel: URL with number: {}", raw_number);
                
                // Clean phone number but keep the plus sign
                let clean_number = raw_number
                    .replace("-", "")
                    .replace(" ", "")
                    .replace("(", "")
                    .replace(")", "");
                
                println!("Cleaned number: {}", clean_number);
                
                // Store phone number and set auto_call flag
                phone_number = clean_number;
                auto_call = true;
                break;
            }
        }
    }

    // Create the main window
    let main_window = WindowDesc::new(build_ui())
        .title(LocalizedString::new("Click-To-Call"))
        .window_size((400.0, 350.0));

    // Set up app state
    let mut app_state = initial_state;
    if auto_call {
        // Only set status message; actual phone number will be set by delegate
        app_state.status_message = format!("Received tel: link. Ready to call: {}", phone_number);
    }
    
    // Create delegate with auto_call info
    let delegate = Delegate {
        auto_call,
        phone_number,
        is_primary,
    };
    
    // Launch the application
    let launcher = AppLauncher::with_window(main_window)
        .delegate(delegate)
        .log_to_console();
    
    launcher.launch(app_state)?;
    Ok(())
}

// Try to connect to a primary instance
fn try_connect_to_primary(socket_path: &PathBuf) -> bool {
    // Remove the socket if it exists but is stale
    if socket_path.exists() {
        if let Ok(mut stream) = UnixStream::connect(socket_path) {
            // Socket exists and connection successful - primary instance is running
            // Send a ping to check if it's alive
            let ping = format!("ping-{}", std::time::SystemTime::now().elapsed().unwrap_or_default().as_secs());
            if stream.write_all(ping.as_bytes()).is_ok() {
                // Successfully connected to primary instance
                return true;
            }
        }
        
        // Socket exists but connection failed - remove the stale socket
        let _ = fs::remove_file(socket_path);
    }
    
    false
}

fn build_ui() -> impl Widget<AppState> {
    // Create label-input pairs for each field
    let domain_label = Label::new("Domain:");
    let domain_input = TextBox::new()
        .with_placeholder("Enter domain")
        .lens(DomainLens)
        .expand_width();
    
    let extension_label = Label::new("Extension:");
    let extension_input = TextBox::new()
        .with_placeholder("Enter extension")
        .lens(ExtensionLens)
        .expand_width();

    let key_label = Label::new("Key:");
    let key_input = TextBox::new()
        .with_placeholder("Enter key")
        .lens(KeyLens)
        .expand_width();
    
    // Auto Answer checkbox
    let auto_answer_checkbox = Checkbox::new("Auto Answer")
        .lens(AutoAnswerLens);
    
    // Phone number input and call button
    let phone_label = Label::new("Phone Number:");
    let phone_input = TextBox::new()
        .with_placeholder("Enter phone number")
        .lens(PhoneNumberLens)
        .expand_width();
    
    // Status message to show feedback
    let status = Label::new(|data: &AppState, _env: &Env| data.status_message.clone());
    
    // Save button
    let save_button = Button::new("Save Settings")
        .on_click(|_ctx, data: &mut AppState, _env| {
            save_preferences(data);
            data.status_message = "Settings saved successfully!".to_string();
        });
    
    // Place Call button
    let place_call_button = Button::new("Place Call")
        .on_click(|ctx, _data: &mut AppState, _env| {
            ctx.submit_command(MAKE_CALL);
        });

    // Create the layout
    let layout = Flex::column()
        .with_child(Flex::row().with_child(domain_label).with_flex_child(domain_input, 1.0))
        .with_spacer(10.0)
        .with_child(Flex::row().with_child(extension_label).with_flex_child(extension_input, 1.0))
        .with_spacer(10.0)
        .with_child(Flex::row().with_child(key_label).with_flex_child(key_input, 1.0))
        .with_spacer(10.0)
        .with_child(auto_answer_checkbox)
        .with_spacer(20.0)
        .with_child(save_button)
        .with_spacer(20.0)
        .with_child(Flex::row().with_child(phone_label).with_flex_child(phone_input, 1.0))
        .with_spacer(10.0)
        .with_child(place_call_button)
        .with_spacer(10.0)
        .with_child(status)
        .padding(20.0);

    layout
}

// Function to save preferences
fn save_preferences(state: &AppState) {
    // Using the dirs crate to get the config directory
    if let Some(config_dir) = dirs::config_dir() {
        let config_path = config_dir.join("click-to-call");
        std::fs::create_dir_all(&config_path).ok();
        
        let prefs_path = config_path.join("preferences.json");
        let json = serde_json::to_string(state).unwrap_or_default();
        
        std::fs::write(prefs_path, json).ok();
    }
}

// Function to load preferences
fn load_preferences() -> AppState {
    let mut state = AppState::default();
    
    if let Some(config_dir) = dirs::config_dir() {
        let prefs_path = config_dir.join("click-to-call").join("preferences.json");
        
        if let Ok(content) = std::fs::read_to_string(prefs_path) {
            if let Ok(loaded_state) = serde_json::from_str::<AppState>(&content) {
                state = loaded_state;
            }
        }
    }
    
    state
}

#[cfg(target_os = "macos")]
extern "C" {
    fn class_addMethod(
        cls: *const objc::runtime::Class,
        name: objc::runtime::Sel,
        imp: extern "C" fn(&objc::runtime::Object, objc::runtime::Sel, *const objc::runtime::Object, *const objc::runtime::Object),
        types: *const libc::c_char,
    ) -> bool;
}