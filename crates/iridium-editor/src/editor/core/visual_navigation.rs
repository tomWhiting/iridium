//! Host-measured movement configuration.
use super::Editor;

impl Editor {
    /// Installs host row measurement while preserving editing transactions.
    pub fn set_visual_navigation(
        &mut self,
        navigation: Option<
            std::sync::Arc<dyn crate::input::keyboard::visual_navigation::VisualNavigation>,
        >,
    ) {
        self.keyboard_handler.set_visual_navigation(navigation);
    }
}
