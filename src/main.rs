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

// Function to show a notification
#[cfg(target_os = "macos")]
fn show_notification(title: &str, message: &str) {
    use objc::{msg_send, sel, sel_impl};
    use objc::runtime::{Class, Object};
    
    println!("Showing notification - Title: '{}', Message: '{}'", title, message);
    
    unsafe {
        // Create a completely new notification center approach
        let app = Class::get("NSApplication").unwrap();
        let app_instance: *mut Object = msg_send![app, sharedApplication];
        
        // Create a user notification
        let notification_class = Class::get("NSUserNotification").unwrap();
        let notification: *mut Object = msg_send![notification_class, new];
        
        // Create NSString objects from Rust strings
        let ns_string_class = Class::get("NSString").unwrap();
        let title_str = std::ffi::CString::new(title).unwrap();
        let message_str = std::ffi::CString::new(message).unwrap();
        let ns_title: *mut Object = msg_send![ns_string_class, stringWithUTF8String:title_str.as_ptr()];
        let ns_message: *mut Object = msg_send![ns_string_class, stringWithUTF8String:message_str.as_ptr()];
        
        // Set properties on the notification
        let _: () = msg_send![notification, setTitle: ns_title];
        let _: () = msg_send![notification, setInformativeText: ns_message];
        
        // Get notification center
        let center_class = Class::get("NSUserNotificationCenter").unwrap();
        let center: *mut Object = msg_send![center_class, defaultUserNotificationCenter];
        
        // Remove existing notifications first
        let _: () = msg_send![center, removeAllDeliveredNotifications];
        
        // Deliver notification
        let _: () = msg_send![center, deliverNotification: notification];
    }
}

#[cfg(not(target_os = "macos"))]
fn show_notification(_title: &str, _message: &str) {
    // Placeholder for other platforms
}

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
                    Ok(response) => {
                        // Check HTTP status code
                        if response.status().is_success() {
                            let success_msg = format!("Call initialized to {}", phone_number);
                            // Show success notification
                            show_notification("Call Initiated", &format!("Calling {}...", phone_number));
                            success_msg
                        } else {
                            let error_msg = format!("Error: HTTP status {}", response.status());
                            // Show error notification
                            show_notification("Call Failed", &format!("Failed to call {}: HTTP status {}", phone_number, response.status()));
                            error_msg
                        }
                    },
                    Err(e) => {
                        let error_msg = format!("Error: {}", e);
                        // Show error notification
                        show_notification("Call Failed", &format!("Failed to call {}: {}", phone_number, e));
                        error_msg
                    },
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
                let app_state = data.clone(); // Clone the current app state
                
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
                                                    // Hide app from dock when processing tel URLs in socket
                                                    #[cfg(target_os = "macos")]
                                                    {
                                                        use objc::{msg_send, sel, sel_impl};
                                                        use objc::runtime::{Class, Object};
                                                        
                                                        unsafe {
                                                            // Don't activate the app when processing tel URLs
                                                            let cls = Class::get("NSApplication").unwrap();
                                                            let app: *mut Object = msg_send![cls, sharedApplication];
                                                            let _: () = msg_send![app, setActivationPolicy:1]; // NSApplicationActivationPolicyAccessory = 1
                                                        }
                                                    }
                                                
                                                    // Extract phone number
                                                    let raw_number = message.split_at(4).1.to_string();
                                                    println!("Socket received tel: URL with number: {}", raw_number);
                                                    
                                                    // Clean phone number but keep the plus sign
                                                    let clean_number = raw_number
                                                        .replace("-", "")
                                                        .replace(" ", "")
                                                        .replace("(", "")
                                                        .replace(")", "");
                                                    
                                                    // If we have valid settings, make call directly without UI
                                                    if !app_state.domain.is_empty() && !app_state.extension.is_empty() {
                                                        make_direct_call(
                                                            &app_state.domain,
                                                            &app_state.extension,
                                                            &app_state.key,
                                                            &clean_number,
                                                            app_state.auto_answer
                                                        );
                                                    } else {
                                                        // Only if settings not configured, send to UI
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
                // On macOS, hide the app from dock when processing tel URLs
                #[cfg(target_os = "macos")]
                {
                    use objc::{msg_send, sel, sel_impl};
                    use objc::runtime::{Class, Object};
                    
                    unsafe {
                        // Don't activate the app when processing tel URLs
                        let cls = Class::get("NSApplication").unwrap();
                        let app: *mut Object = msg_send![cls, sharedApplication];
                        let _: () = msg_send![app, setActivationPolicy:1]; // NSApplicationActivationPolicyAccessory = 1
                    }
                }
                
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
                    // Store the phone number in data for the call
                    data.phone_number = clean_number.clone();
                    data.status_message = format!("Processing tel: URL: {}", raw_number);
                    
                    // Don't bring window to front, just initiate the call silently
                    
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

// Function to make a direct call without involving the UI
fn make_direct_call(domain: &str, extension: &str, key: &str, phone_number: &str, auto_answer: bool) {
    println!("Making direct call to {} without showing UI", phone_number);
    
    // Clone data we need for the HTTP request
    let domain = domain.to_string();
    let extension = extension.to_string();
    let key = key.to_string();
    let phone_number = phone_number.to_string();
    
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
        match Client::new().get(url_str).send() {
            Ok(response) => {
                // Check HTTP status code
                if response.status().is_success() {
                    show_notification("Call Initiated", &format!("Calling {}...", phone_number));
                    println!("Call initialized to {}", phone_number);
                } else {
                    show_notification("Call Failed", &format!("Failed to call {}: HTTP status {}", phone_number, response.status()));
                    println!("Error: HTTP status {}", response.status());
                }
            },
            Err(e) => {
                show_notification("Call Failed", &format!("Failed to call {}: {}", phone_number, e));
                println!("Error: {}", e);
            },
        };
    });
}

#[cfg(target_os = "macos")]
fn hide_app_from_dock() {
    use objc::{msg_send, sel, sel_impl};
    use objc::runtime::{Class, Object};
    
    unsafe {
        // Get the shared application
        let cls = Class::get("NSApplication").unwrap();
        let app: *mut Object = msg_send![cls, sharedApplication];
        
        // Set activation policy to prohibit the app from showing in the Dock
        let _: () = msg_send![app, setActivationPolicy:1]; // NSApplicationActivationPolicyAccessory = 1
    }
}

#[cfg(not(target_os = "macos"))]
fn hide_app_from_dock() {
    // No-op for non-macOS platforms
}

fn main() -> Result<(), PlatformError> {
    // Check if the app is already running
    let socket_path = get_socket_path();
    let is_primary = !try_connect_to_primary(&socket_path);
    
    // Print all args for debugging
    println!("Received arguments: {:?}", env::args().collect::<Vec<_>>());
    
    // On macOS, the URL is passed through the process arguments
    let args: Vec<String> = env::args().collect();
    let mut has_tel_url = false;
    let mut tel_number = String::new();
    
    // Check for tel: URL in app arguments
    if args.len() > 1 {
        // Look for tel: URL in all arguments
        for arg in &args[1..] {
            println!("Checking arg: {}", arg);
            
            // Check for tel: prefix (case insensitive)
            let arg_lower = arg.to_lowercase();
            if arg_lower.starts_with("tel:") {
                has_tel_url = true;
                
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
                tel_number = clean_number;
                break;
            }
        }
    }
    
    // If we're handling a tel: URL and this is a primary instance, hide from dock
    if has_tel_url && is_primary {
        hide_app_from_dock();
    }
    
    // Handle the tel: URL if present
    if has_tel_url {
        // If this is not the primary instance, try to send the URL to the primary instance
        if !is_primary {
            if let Ok(mut stream) = UnixStream::connect(&socket_path) {
                let url = format!("tel:{}", tel_number);
                if stream.write_all(url.as_bytes()).is_ok() {
                    // Successfully sent to primary instance, exit this one
                    println!("Sent URL to primary instance and exiting");
                    return Ok(());
                }
            } 
            // If can't connect to socket, try to spawn a background instance
            else {
                // Try to spawn a background instance
                #[cfg(target_os = "macos")]
                {
                    use std::process::Command;
                    
                    // Determine the path to the current executable
                    if let Ok(current_exe) = std::env::current_exe() {
                        println!("Spawning background instance: {:?}", current_exe);
                        // Launch the app as a background process
                        let _ = Command::new("open")
                            .arg("-g") // -g makes it open in the background
                            .arg(current_exe)
                            .spawn();
                        
                        // Wait a moment for the process to start
                        std::thread::sleep(std::time::Duration::from_millis(1000));
                        
                        // Try to connect to the socket again
                        if let Ok(mut stream) = UnixStream::connect(&socket_path) {
                            let url = format!("tel:{}", tel_number);
                            if stream.write_all(url.as_bytes()).is_ok() {
                                println!("Sent URL to newly spawned instance and exiting");
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }
        
        // Process the tel: URL directly
        let app_state = load_preferences();
        
        // If domain and extension are configured, make call without showing the UI
        if !app_state.domain.is_empty() && !app_state.extension.is_empty() {
            // Make a direct call without showing the UI
            make_direct_call(&app_state.domain, &app_state.extension, &app_state.key, &tel_number, app_state.auto_answer);
            return Ok(());
        }
        
        // If we get here, we need to show the UI to configure settings
        println!("Settings not configured, need to show UI");
    }
    
    // Register apple event handler for MacOS URL scheme (only for primary instance)
    #[cfg(target_os = "macos")]
    if is_primary {
        configure_apple_event_handler();
    }

    // Create the main window
    let main_window = WindowDesc::new(build_ui())
        .title(LocalizedString::new("Click-To-Call"))
        .window_size((400.0, 350.0));

    // Set up app state
    let mut initial_state = load_preferences();
    
    // Create delegate with proper flags
    let delegate = Delegate {
        auto_call: false,
        phone_number: String::new(),
        is_primary,
    };
    
    // Launch the application
    let launcher = AppLauncher::with_window(main_window)
        .delegate(delegate)
        .log_to_console();
    
    launcher.launch(initial_state)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn configure_apple_event_handler() {
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
                        // Hide the app from dock when processing tel URLs
                        hide_app_from_dock();
                        
                        // Try to connect to existing instance
                        let socket_path = get_socket_path();
                        if let Ok(mut stream) = UnixStream::connect(&socket_path) {
                            // If connection succeeds, send the URL and we're done
                            if stream.write_all(url.as_bytes()).is_ok() {
                                println!("Sent URL to existing instance");
                                return;
                            }
                        }
                        
                        // If we couldn't connect, try to handle it directly
                        if url.starts_with("tel:") {
                            // Extract phone number
                            let raw_number = url.split_at(4).1.to_string();
                            
                            // Clean phone number but keep the plus sign
                            let clean_number = raw_number
                                .replace("-", "")
                                .replace(" ", "")
                                .replace("(", "")
                                .replace(")", "");
                            
                            // Load preferences and check if we can make a direct call
                            if let Some(config_dir) = dirs::config_dir() {
                                let prefs_path = config_dir.join("click-to-call").join("preferences.json");
                                
                                if let Ok(content) = std::fs::read_to_string(prefs_path) {
                                    if let Ok(app_state) = serde_json::from_str::<AppState>(&content) {
                                        if !app_state.domain.is_empty() && !app_state.extension.is_empty() {
                                            // Make the call without showing UI
                                            let domain = app_state.domain.clone();
                                            let extension = app_state.extension.clone();
                                            let key = app_state.key.clone();
                                            let auto_answer = app_state.auto_answer;
                                            
                                            std::thread::spawn(move || {
                                                // Directly call the API endpoint
                                                make_direct_call(&domain, &extension, &key, &clean_number, auto_answer);
                                            });
                                            return;
                                        }
                                    }
                                }
                            }
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
    // We still need this for URL handling, but not for notifications
}