//! AppKit target/action bridge for dispatching menu commands.

#[cfg(not(target_os = "macos"))]
pub struct CommandTarget;

#[cfg(target_os = "macos")]
mod macos {
    use log::warn;
    use objc2::{
        define_class, msg_send,
        rc::Retained,
        runtime::{AnyObject, Sel},
        sel, DefinedClass, MainThreadOnly,
    };
    use objc2_foundation::{MainThreadMarker, NSInteger, NSObject, NSObjectProtocol};
    use winit::event_loop::EventLoopProxy;

    use crate::{
        app::AppCommand,
        menu_bar::{
            command_from_tag, MENU_TAG_HOVER_INTENSITY, MENU_TAG_MONITOR_BEHAVIOR,
            MENU_TAG_MOVEMENT_SPEED, MENU_TAG_PERSONALITY, MENU_TAG_SCALE,
        },
        pet::Personality,
        settings::MonitorBehavior,
    };

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "HappyCappyCommandTarget"]
        #[thread_kind = MainThreadOnly]
        #[ivars = EventLoopProxy<AppCommand>]
        pub struct CommandTarget;

        unsafe impl NSObjectProtocol for CommandTarget {}

        impl CommandTarget {
            #[unsafe(method(dispatchCommand:))]
            fn dispatch_command(&self, sender: Option<&AnyObject>) {
                let Some(sender) = sender else {
                    return;
                };

                let tag: NSInteger = unsafe { msg_send![sender, tag] };
                if let Some(command) = command_from_tag(tag as isize) {
                    self.send_command(command);
                }
            }

            #[unsafe(method(dispatchSettingsValue:))]
            fn dispatch_settings_value(&self, sender: Option<&AnyObject>) {
                let Some(sender) = sender else {
                    return;
                };

                let tag: NSInteger = unsafe { msg_send![sender, tag] };
                let command = match tag as isize {
                    MENU_TAG_PERSONALITY => {
                        let selected_segment: NSInteger =
                            unsafe { msg_send![sender, selectedSegment] };
                        match selected_segment {
                            0 => Some(AppCommand::SetPersonality(Personality::Calm)),
                            1 => Some(AppCommand::SetPersonality(Personality::Cheerful)),
                            2 => Some(AppCommand::SetPersonality(Personality::Lively)),
                            _ => None,
                        }
                    }
                    MENU_TAG_SCALE => Some(AppCommand::SetScale(read_double_value(sender))),
                    MENU_TAG_MOVEMENT_SPEED => {
                        Some(AppCommand::SetMovementSpeed(read_double_value(sender)))
                    }
                    MENU_TAG_HOVER_INTENSITY => {
                        Some(AppCommand::SetHoverIntensity(read_double_value(sender)))
                    }
                    MENU_TAG_MONITOR_BEHAVIOR => {
                        let selected_segment: NSInteger =
                            unsafe { msg_send![sender, selectedSegment] };
                        match selected_segment {
                            0 => Some(AppCommand::SetMonitorBehavior(
                                MonitorBehavior::CurrentDisplay,
                            )),
                            1 => Some(AppCommand::SetMonitorBehavior(
                                MonitorBehavior::PrimaryDisplay,
                            )),
                            _ => None,
                        }
                    }
                    _ => None,
                };

                if let Some(command) = command {
                    self.send_command(command);
                }
            }
        }
    );

    impl CommandTarget {
        pub fn new(mtm: MainThreadMarker, proxy: EventLoopProxy<AppCommand>) -> Retained<Self> {
            let this = mtm.alloc().set_ivars(proxy);
            unsafe { msg_send![super(this), init] }
        }

        pub fn command_selector() -> Sel {
            sel!(dispatchCommand:)
        }

        pub fn settings_value_selector() -> Sel {
            sel!(dispatchSettingsValue:)
        }

        fn send_command(&self, command: AppCommand) {
            if let Err(error) = self.ivars().send_event(command) {
                warn!("failed to dispatch app command: {error}");
            }
        }
    }

    fn read_double_value(sender: &AnyObject) -> f32 {
        let value: f64 = unsafe { msg_send![sender, doubleValue] };
        value as f32
    }
}

#[cfg(target_os = "macos")]
pub use macos::CommandTarget;
