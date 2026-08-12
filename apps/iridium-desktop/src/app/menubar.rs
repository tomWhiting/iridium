//! The macOS menu bar's host seam: installing it, and running what it chose.
//!
//! The **ruled set** — which menus there are and which commands in each — is
//! [`crate::menubar`], and it is pure Rust the ten gates cover. This file is
//! the half that talks to `AppKit`, and ⚠️ **no test in this repository can
//! reach it**: it needs a running `NSApplication`, which `cargo test` does not
//! have. That boundary is why D-1 ruled against key equivalents in this slice
//! (`docs/IN-FLIGHT-108-menu-bar.md`) — moving this face's most-used chords
//! onto an untested path is a trade worth refusing.
//!
//! # winit's menu bar is kept, not replaced
//!
//! winit installs an `NSMenu` of its own unless a host calls
//! `with_default_menu(false)`, and this face does not. That menu holds the
//! application menu — *About*, *Services*, *Hide*, *Quit* — and **nothing
//! else**, which is precisely why there was no way in for a hand that does not
//! know the chords. Menus are **appended** to it. Replacing it would lose
//! *Quit* and *Hide* for nothing.
//!
//! # ⛔ No proxy, no menu
//!
//! [`install_menubar`](DesktopApp::install_menubar) does nothing without an
//! [`EventLoopProxy`], and that is deliberate rather than defensive: without
//! one, every item would be drawn, clickable and dead. A menu that cannot run
//! what it offers is furniture promising what the routing does not deliver —
//! the same failure the sidebar refused in #112d step 6. Better no menu than a
//! menu that lies.

use winit::event_loop::EventLoopProxy;

use super::state::{DesktopApp, Flow};
use crate::menubar::{self, MenuCommand};
use crate::verbs::Availability;

impl DesktopApp {
    /// Hands this session the channel its menu bar will answer through.
    ///
    /// Called from [`crate::run`], because only an `EventLoop` can make a
    /// proxy and the app is built before one exists. A session that is never
    /// given one gets **no menu bar**, which is the honest outcome — see the
    /// module docs.
    pub fn attach_menu_proxy(&mut self, proxy: EventLoopProxy<MenuCommand>) {
        self.menu_proxy = Some(proxy);
    }

    /// Builds this session's menus and hangs them off the application's menu
    /// bar, once.
    ///
    /// Called from `resumed`, inside the guard that runs a single time: a menu
    /// bar installed twice would be two *File* menus.
    ///
    /// The rows are resolved against the registry **here**, at install, rather
    /// than per open: what a menu offers must not change under the hand
    /// reaching for it, and the only thing that varies between sessions —
    /// which commands are registered — is fixed by the time the loop resumes.
    pub(super) fn install_menubar(&self, proxy: &EventLoopProxy<MenuCommand>) {
        let Some(editor) = self.workspace.active_editor() else {
            return;
        };
        // Every row enabled: the greying push is step 4 of this task, and
        // until it lands a row that cannot run reports so through
        // `run_chosen_command`'s existing refusal rather than being silently
        // dim. An honest refusal beats a grey row that has not been earned.
        let menus = menubar::menus(editor, Availability::OPEN);
        platform::install(&menus, proxy);
    }

    /// What the menu bar may offer to run right now.
    ///
    /// ⭐ **This is the testable half of the greying push**, and it is split
    /// out for that reason: what the answer *is* can be proven by `cargo
    /// test`, while pushing it into `AppKit` cannot be.
    ///
    /// `inert` names the four panels that swallow a key outright — read off
    /// [`press`](DesktopApp::press)'s own branch order rather than guessed at,
    /// so the menu bar and the keyboard cannot disagree about what modal
    /// means. While one of them is up, nothing outside it runs.
    ///
    /// ⚠️ **Explorer focus is deliberately not modality.** The explorer takes
    /// keys when it has focus, but it is a focus and not a panel that owns the
    /// session: the active editor is still well defined, and a menu verb aimed
    /// at it still means what it says. Greying the whole bar because a side
    /// panel holds the caret would be greying on a technicality.
    ///
    /// A session with no editor at all is inert, because there is nothing for
    /// any of these verbs to act on.
    pub(super) fn menu_availability(&self) -> Availability {
        let Some(editor) = self.workspace.active_editor() else {
            return Availability {
                read_only: false,
                inert: true,
            };
        };
        Availability {
            read_only: editor.state().read_only,
            inert: self.prompt.is_some()
                || self.menu.is_some()
                || self.palette_open
                || self.history_open,
        }
    }

    /// Pushes the enabled set onto the installed menu items, when it moved.
    ///
    /// Called from `about_to_wait`: the one moment per turn when the loop is
    /// about to go to sleep, and so the one moment when the menu bar can be
    /// reached by a mouse while nothing else is able to change it.
    ///
    /// ⭐ **Nothing is touched when the answer has not moved.** Enabledness is
    /// a pure function of [`Availability`] and whether a command writes, and
    /// the registry does not change mid-session — so an unchanged availability
    /// means an unchanged bar, and the common case costs one comparison of two
    /// `bool`s. Without the guard this would cross into `AppKit` on every
    /// keystroke to set values that were already set.
    pub(super) fn refresh_menubar(&mut self) {
        let availability = self.menu_availability();
        if self.last_menu_availability == Some(availability) {
            return;
        }
        self.last_menu_availability = Some(availability);
        let Some(editor) = self.workspace.active_editor() else {
            return;
        };
        let commands = editor.commands();
        platform::set_enabled(&|id| {
            // A command the registry has lost is left as it was rather than
            // greyed: it cannot happen mid-session, and inventing an answer
            // for it would be inventing state.
            commands
                .get(id.as_str())
                .is_none_or(|meta| availability.allows(meta.mutates_document()))
        });
    }

    /// Runs a command the menu bar chose.
    ///
    /// One line, and that is the point: the menu reaches the document by the
    /// same route every other verb in this face takes, so nothing about a
    /// menu-run command can differ from the same command run from a chord.
    pub(super) fn run_menu_command(&mut self, chosen: &MenuCommand) -> Flow {
        // A message describes the input before this one.
        self.message = None;
        let flow = self.run_chosen_command(&chosen.0);
        self.request_redraw();
        flow
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::cell::RefCell;

    use iridium_editor::CommandId;
    use objc2::rc::Retained;
    use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
    use objc2_app_kit::{NSApplication, NSMenu, NSMenuItem};
    use objc2_foundation::{NSObjectProtocol, NSString};
    use winit::event_loop::EventLoopProxy;

    use crate::menubar::{Menu, MenuCommand};

    thread_local! {
        /// Every item this face put on the bar, with the command it runs.
        ///
        /// ⭐ **Thread-local rather than a field on `DesktopApp`**, for two
        /// reasons that point the same way. There is exactly *one* menu bar
        /// per process — `NSApp.mainMenu` is global — so a per-session field
        /// would be modelling a one-to-many that does not exist. And
        /// `Retained<NSMenuItem>` is main-thread-only; keeping it here means
        /// a call from any other thread finds an empty list and does nothing,
        /// which is the correct answer, rather than needing a runtime check
        /// that could be forgotten.
        static INSTALLED: RefCell<Vec<(CommandId, Retained<NSMenuItem>)>> =
            const { RefCell::new(Vec::new()) };
    }

    /// What the target needs to answer a click.
    struct TargetIvars {
        /// The channel back into the winit event loop.
        proxy: EventLoopProxy<MenuCommand>,
        /// Every command the bar can run, indexed by the tag its item carries.
        ///
        /// ⚠️ **A tag rather than the item's title.** Titles are shown to a
        /// person and may be translated or reworded; a tag is an index this
        /// code set itself and nothing else can change.
        commands: Vec<MenuCommand>,
    }

    define_class!(
        // SAFETY:
        // - `NSObject` imposes no subclassing requirements.
        // - This type does not implement `Drop`.
        // - `MainThreadOnly` because the only thing that ever sends it a
        //   message is `AppKit`'s menu tracking, which is main-thread by
        //   construction, and because it is built from `resumed`.
        #[unsafe(super(objc2_foundation::NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "IridiumMenuTarget"]
        #[ivars = TargetIvars]
        struct MenuTarget;

        impl MenuTarget {
            /// The action every item on this face's menus points at.
            ///
            /// ⛔ **Nothing here touches the editor.** The app is borrowed by
            /// `run_app` for the loop's whole lifetime, so this hands the id
            /// to the proxy and returns; the loop runs it on its next turn.
            #[unsafe(method(iridiumMenuAction:))]
            fn action(&self, sender: &NSMenuItem) {
                let tag = sender.tag();
                let Ok(index) = usize::try_from(tag) else {
                    // A negative tag is not one this code set. Silence is the
                    // only honest answer — there is no command to run and no
                    // channel here to report on.
                    return;
                };
                let Some(command) = self.ivars().commands.get(index) else {
                    return;
                };
                // A closed event loop is the ordinary way this fails: the
                // window went away between the click and the dispatch. There
                // is nothing left to tell, and no session to tell it to.
                let _ = self.proxy_send(command.clone());
            }
        }

        unsafe impl NSObjectProtocol for MenuTarget {}
    );

    impl MenuTarget {
        /// Builds the target, holding the proxy and the command table.
        #[expect(
            unsafe_code,
            reason = "the only way to run an Objective-C superclass \
                      initialiser; `set_ivars` has already stored the fields, \
                      and `init` on NSObject is the interface objc2 documents \
                      for finishing a `define_class!` type"
        )]
        fn new(mtm: MainThreadMarker, ivars: TargetIvars) -> Retained<Self> {
            let this = Self::alloc(mtm).set_ivars(ivars);
            unsafe { msg_send![super(this), init] }
        }

        /// Hands one command to the event loop.
        fn proxy_send(
            &self,
            command: MenuCommand,
        ) -> Result<(), winit::event_loop::EventLoopClosed<MenuCommand>> {
            self.ivars().proxy.send_event(command)
        }
    }

    /// Appends `menus` to the application's existing menu bar.
    ///
    /// Does nothing when `AppKit` is reachable from somewhere that is not the
    /// main thread, or when there is no menu bar to append to — neither is a
    /// state this face reaches, and neither is worth taking the process down
    /// for. `MainThreadMarker` is how the first is *asked* rather than
    /// assumed, exactly as [`crate::dialog`] asks it.
    #[expect(
        unsafe_code,
        reason = "AppKit's menu item constructor and `setTarget:` are unsafe \
                  because a bad selector or a dead target is a crash at click \
                  time; both are answered here — the selector is one the class \
                  above implements, and the target outlives the menu bar \
                  because it is deliberately leaked. There is no safe wrapper \
                  for hanging a menu off NSApp, and a menu bar is what #108 is"
    )]
    pub(super) fn install(menus: &[Menu], proxy: &EventLoopProxy<MenuCommand>) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let app = NSApplication::sharedApplication(mtm);
        let Some(bar) = app.mainMenu() else {
            return;
        };

        // The table the tags index into, built in the same walk that builds
        // the items so an item's tag and its command cannot come apart.
        let mut commands: Vec<MenuCommand> = Vec::new();
        for menu in menus {
            for row in menu.commands() {
                commands.push(MenuCommand(row.command.clone()));
            }
        }
        let target = MenuTarget::new(
            mtm,
            TargetIvars {
                proxy: proxy.clone(),
                commands,
            },
        );

        let mut installed: Vec<(CommandId, Retained<NSMenuItem>)> = Vec::new();
        let mut tag: isize = 0;
        for menu in menus {
            let title = NSString::from_str(menu.title);
            let submenu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &title);
            // ⚠️ Off, because this face decides what is enabled. Left on,
            // AppKit walks the responder chain asking whether each action is
            // implemented — and it is implemented by an object that always
            // says yes, which would make the flag decide nothing while
            // looking like it decided something.
            submenu.setAutoenablesItems(false);

            for row in &menu.rows {
                let Some(row) = row.as_ref() else {
                    submenu.addItem(&NSMenuItem::separatorItem(mtm));
                    continue;
                };
                // SAFETY: the selector is one `MenuTarget` implements, and
                // the target set below is what receives it.
                //
                // D-1: the action is ours, the key equivalent is **empty**. An
                // `NSMenuItem` displays a chord by *claiming* it, and claiming
                // ⌘S would move this face's most-used keys onto this
                // untestable path. See the module docs.
                let item = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(mtm),
                        &NSString::from_str(row.label),
                        Some(sel!(iridiumMenuAction:)),
                        &NSString::from_str(""),
                    )
                };
                item.setTag(tag);
                item.setEnabled(row.enabled);
                installed.push((row.command.clone(), item.clone()));
                // SAFETY: `target` outlives the menu bar — see the leak at the
                // foot of this function — and `iridiumMenuAction:` is a
                // selector the class above actually implements.
                unsafe { item.setTarget(Some(&target)) };
                submenu.addItem(&item);
                tag += 1;
            }

            // A submenu hangs off an item on the bar, and that item's own
            // title is what the bar shows.
            // SAFETY: no action and no key equivalent — this item only holds
            // the submenu, and clicking the word on the bar opens it rather
            // than running anything.
            let holder = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    &title,
                    None,
                    &NSString::from_str(""),
                )
            };
            holder.setSubmenu(Some(&submenu));
            bar.addItem(&holder);
        }

        INSTALLED.with_borrow_mut(|items| *items = installed);

        // ⭐ The target is deliberately leaked. `NSMenuItem`'s target is a
        // **weak** reference — AppKit does not retain it — so a target dropped
        // at the end of this function would leave every item pointing at freed
        // memory, and the first click would be a crash. It lives exactly as
        // long as the menu bar does, which is the whole process.
        std::mem::forget(target);
    }

    /// Sets every installed item's enabled flag from `is_enabled`.
    ///
    /// Does nothing before [`install`] has run, and nothing off the main
    /// thread — in both cases there are no items to walk, which is the honest
    /// answer rather than a guard bolted on.
    pub(super) fn set_enabled(is_enabled: &dyn Fn(&CommandId) -> bool) {
        INSTALLED.with_borrow(|items| {
            for (command, item) in items {
                item.setEnabled(is_enabled(command));
            }
        });
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use winit::event_loop::EventLoopProxy;

    use crate::menubar::{Menu, MenuCommand};

    /// There are no items to enable, because there is no menu bar.
    pub(super) fn set_enabled(_is_enabled: &dyn Fn(&iridium_editor::CommandId) -> bool) {}

    /// No menu bar is built.
    ///
    /// Unlike [`crate::dialog`], this needs no `Unsupported` channel to say so:
    /// an absent menu is not something a user can press and get silence from.
    /// There is simply nothing on the bar, and the palette is the way in — the
    /// same way in the terminal face has.
    pub(super) fn install(_menus: &[Menu], _proxy: &EventLoopProxy<MenuCommand>) {}
}
