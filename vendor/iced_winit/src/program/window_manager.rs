use crate::core::mouse;
use crate::core::window::Id;
use crate::core::{Point, Size};
use crate::graphics::Compositor;
use crate::program::{DefaultStyle, Program, State};

use std::collections::BTreeMap;
use std::sync::Arc;
use winit::monitor::MonitorHandle;

#[allow(missing_debug_implementations)]
pub struct WindowManager<P, C>
where
    P: Program,
    C: Compositor<Renderer = P::Renderer>,
    P::Theme: DefaultStyle,
{
    aliases: BTreeMap<winit::window::WindowId, Id>,
    entries: BTreeMap<Id, Window<P, C>>,
}

impl<P, C> WindowManager<P, C>
where
    P: Program,
    C: Compositor<Renderer = P::Renderer>,
    P::Theme: DefaultStyle,
{
    pub fn new() -> Self {
        Self {
            aliases: BTreeMap::new(),
            entries: BTreeMap::new(),
        }
    }

    pub fn insert(
        &mut self,
        id: Id,
        window: Arc<winit::window::Window>,
        application: &P,
        compositor: &mut C,
        exit_on_close_request: bool,
    ) -> &mut Window<P, C> {
        let state = State::new(application, id, &window);
        let viewport_version = state.viewport_version();
        let renderer = compositor.create_renderer();

        let _ = self.aliases.insert(window.id(), id);

        let _ = self.entries.insert(
            id,
            Window {
                raw: window,
                state,
                viewport_version,
                exit_on_close_request,
                surface: None,
                renderer,
                mouse_interaction: mouse::Interaction::None,
            },
        );

        self.entries
            .get_mut(&id)
            .expect("Get window that was just inserted")
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn first(&self) -> Option<&Window<P, C>> {
        self.entries.first_key_value().map(|(_id, window)| window)
    }

    pub fn iter_mut(
        &mut self,
    ) -> impl Iterator<Item = (Id, &mut Window<P, C>)> {
        self.entries.iter_mut().map(|(k, v)| (*k, v))
    }

    pub fn get(&self, id: Id) -> Option<&Window<P, C>> {
        self.entries.get(&id)
    }

    pub fn get_mut(&mut self, id: Id) -> Option<&mut Window<P, C>> {
        self.entries.get_mut(&id)
    }

    pub fn get_mut_alias(
        &mut self,
        id: winit::window::WindowId,
    ) -> Option<(Id, &mut Window<P, C>)> {
        let id = self.aliases.get(&id).copied()?;

        Some((id, self.get_mut(id)?))
    }

    pub fn last_monitor(&self) -> Option<MonitorHandle> {
        self.entries.values().last()?.raw.current_monitor()
    }

    pub fn remove(&mut self, id: Id) -> Option<Window<P, C>> {
        let window = self.entries.remove(&id)?;
        let _ = self.aliases.remove(&window.raw.id());

        Some(window)
    }
}

impl<P, C> Default for WindowManager<P, C>
where
    P: Program,
    C: Compositor<Renderer = P::Renderer>,
    P::Theme: DefaultStyle,
{
    fn default() -> Self {
        Self::new()
    }
}

#[allow(missing_debug_implementations)]
pub struct Window<P, C>
where
    P: Program,
    C: Compositor<Renderer = P::Renderer>,
    P::Theme: DefaultStyle,
{
    pub raw: Arc<winit::window::Window>,
    pub state: State<P>,
    pub viewport_version: u64,
    pub exit_on_close_request: bool,
    pub mouse_interaction: mouse::Interaction,
    pub surface: Option<C::Surface>,
    pub renderer: P::Renderer,
}

impl<P, C> Window<P, C>
where
    P: Program,
    C: Compositor<Renderer = P::Renderer>,
    P::Theme: DefaultStyle,
{
    /// Occlusion alone does not suspend a visible window.
    pub fn graphics_are_suspended(&self) -> bool {
        !self.raw.is_visible().unwrap_or(true)
            || self.raw.is_minimized().unwrap_or(false)
    }

    /// Keep the native window and UI state; drop backend pixels and caches.
    pub fn release_graphics(&mut self, compositor: &C) {
        self.surface = None;
        self.renderer = compositor.create_renderer();
    }

    /// Create a fresh, correctly configured surface after show/restore.
    pub fn ensure_surface(&mut self, compositor: &mut C) -> bool {
        let size = self.state.physical_size();
        if size.width == 0 || size.height == 0 {
            return false;
        }
        if self.surface.is_none() {
            self.surface = Some(compositor.create_surface(
                self.raw.clone(),
                size.width,
                size.height,
            ));
        } else if self.viewport_version != self.state.viewport_version() {
            compositor.configure_surface(
                self.surface.as_mut().expect("Existing surface"),
                size.width,
                size.height,
            );
        }
        self.viewport_version = self.state.viewport_version();
        true
    }

    pub fn position(&self) -> Option<Point> {
        self.raw
            .inner_position()
            .ok()
            .map(|position| position.to_logical(self.raw.scale_factor()))
            .map(|position| Point {
                x: position.x,
                y: position.y,
            })
    }

    pub fn size(&self) -> Size {
        let size = self.raw.inner_size().to_logical(self.raw.scale_factor());

        Size::new(size.width, size.height)
    }
}
