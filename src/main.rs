use druid::widget::{Button, Checkbox, Flex, Label, TextBox};
use druid::{AppLauncher, Data, Env, Lens, LocalizedString, PlatformError, Widget, WidgetExt, WindowDesc};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use url::Url;
use std::thread;

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

fn main() -> Result<(), PlatformError> {
    // Create the main window
    let main_window = WindowDesc::new(build_ui())
        .title(LocalizedString::new("Click to Call - FusionPBX"))
        .window_size((400.0, 350.0));

    // Load saved preferences or use defaults
    let initial_state = load_preferences();

    // Launch the application
    AppLauncher::with_window(main_window)
        .log_to_console()
        .launch(initial_state)?;
    Ok(())
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
        .on_click(|ctx, data: &mut AppState, _env| {
            // Make sure we have the necessary data
            if data.domain.is_empty() || data.extension.is_empty() || data.phone_number.is_empty() {
                data.status_message = "Error: Missing domain, extension or phone number".to_string();
                return;
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