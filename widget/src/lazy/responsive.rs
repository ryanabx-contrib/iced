use crate::core::event::{self, Event};
use crate::core::layout::{self, Layout};
use crate::core::mouse;
use crate::core::overlay;
use crate::core::renderer;
use crate::core::widget::tree::{self, Tree};
use crate::core::{
    self, Clipboard, Element, Length, Point, Rectangle, Shell, Size, Vector,
    Widget,
};
use crate::horizontal_space;
use crate::runtime::overlay::Nested;

use iced_renderer::core::widget::Operation;
use ouroboros::self_referencing;
use std::cell::{RefCell, RefMut};
use std::marker::PhantomData;
use std::ops::Deref;

/// A widget that is aware of its dimensions.
///
/// A [`Responsive`] widget will always try to fill all the available space of
/// its parent.
#[cfg(feature = "lazy")]
#[allow(missing_debug_implementations)]
pub struct Responsive<
    'a,
    Message,
    Theme = crate::Theme,
    Renderer = crate::Renderer,
> {
    view: Box<dyn Fn(Size) -> Element<'a, Message, Theme, Renderer> + 'a>,
    content: RefCell<Content<'a, Message, Theme, Renderer>>,
}

impl<'a, Message, Theme, Renderer> Responsive<'a, Message, Theme, Renderer>
where
    Renderer: core::Renderer,
{
    /// Creates a new [`Responsive`] widget with a closure that produces its
    /// contents.
    ///
    /// The `view` closure will be provided with the current [`Size`] of
    /// the [`Responsive`] widget and, therefore, can be used to build the
    /// contents of the widget in a responsive way.
    pub fn new(
        view: impl Fn(Size) -> Element<'a, Message, Theme, Renderer> + 'a,
    ) -> Self {
        Self {
            view: Box::new(view),
            content: RefCell::new(Content {
                size: Size::ZERO,
                layout: None,
                element: Element::new(horizontal_space().width(0)),
            }),
        }
    }
}

struct Content<'a, Message, Theme, Renderer> {
    size: Size,
    layout: Option<layout::Node>,
    element: Element<'a, Message, Theme, Renderer>,
}

impl<'a, Message, Theme, Renderer> Content<'a, Message, Theme, Renderer>
where
    Renderer: core::Renderer,
{
    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer) {
        if self.layout.is_none() {
            self.layout = Some(self.element.as_widget().layout(
                tree,
                renderer,
                &layout::Limits::new(Size::ZERO, self.size),
            ));
        }
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        new_size: Size,
        view: &dyn Fn(Size) -> Element<'a, Message, Theme, Renderer>,
    ) {
        let is_tree_empty =
            tree.tag == tree::Tag::stateless() && tree.children.is_empty();

        if !is_tree_empty && self.size == new_size {
            return;
        }

        self.element = view(new_size);
        self.size = new_size;
        self.layout = None;

        tree.diff(&mut self.element);
    }

    fn resolve<R, T>(
        &mut self,
        tree: &mut Tree,
        renderer: R,
        layout: Layout<'_>,
        view: &dyn Fn(Size) -> Element<'a, Message, Theme, Renderer>,
        f: impl FnOnce(
            &mut Tree,
            R,
            Layout<'_>,
            &mut Element<'a, Message, Theme, Renderer>,
        ) -> T,
    ) -> T
    where
        R: Deref<Target = Renderer>,
    {
        self.update(tree, layout.bounds().size(), view);
        self.layout(tree, renderer.deref());

        let content_layout = Layout::with_offset(
            layout.position() - Point::ORIGIN,
            self.layout.as_ref().unwrap(),
        );

        f(tree, renderer, content_layout, &mut self.element)
    }
}

struct State {
    tree: RefCell<Tree>,
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Responsive<'a, Message, Theme, Renderer>
where
    Renderer: core::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            tree: RefCell::new(Tree::empty()),
        })
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fill,
            height: Length::Fill,
        }
    }

    fn layout(
        &self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn operate(
        &self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn crate::core::widget::Operation,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let mut content = self.content.borrow_mut();

        content.resolve(
            &mut state.tree.borrow_mut(),
            renderer,
            layout,
            &self.view,
            |tree, renderer, layout, element| {
                element
                    .as_widget()
                    .operate(tree, layout, renderer, operation);
            },
        );
    }

    fn on_event(
        &mut self,
        tree: &mut Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) -> event::Status {
        let state = tree.state.downcast_mut::<State>();
        let mut content = self.content.borrow_mut();

        let mut local_messages = vec![];
        let mut local_shell = Shell::new(&mut local_messages);

        let status = content.resolve(
            &mut state.tree.borrow_mut(),
            renderer,
            layout,
            &self.view,
            |tree, renderer, layout, element| {
                element.as_widget_mut().on_event(
                    tree,
                    event,
                    layout,
                    cursor,
                    renderer,
                    clipboard,
                    &mut local_shell,
                    viewport,
                )
            },
        );

        if local_shell.is_layout_invalid() {
            content.layout = None;
        }

        shell.merge(local_shell, std::convert::identity);

        status
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let mut content = self.content.borrow_mut();

        content.resolve(
            &mut state.tree.borrow_mut(),
            renderer,
            layout,
            &self.view,
            |tree, renderer, layout, element| {
                element.as_widget().draw(
                    tree, renderer, theme, style, layout, cursor, viewport,
                );
            },
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        let mut content = self.content.borrow_mut();

        content.resolve(
            &mut state.tree.borrow_mut(),
            renderer,
            layout,
            &self.view,
            |tree, renderer, layout, element| {
                element
                    .as_widget()
                    .mouse_interaction(tree, layout, cursor, viewport, renderer)
            },
        )
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        use std::ops::DerefMut;

        let state = tree.state.downcast_ref::<State>();

        let overlay = OverlayBuilder {
            content: self.content.borrow_mut(),
            tree: state.tree.borrow_mut(),
            types: PhantomData,
            overlay_builder: |content: &mut RefMut<
                '_,
                Content<'_, _, _, _>,
            >,
                              tree| {
                content.update(tree, layout.bounds().size(), &self.view);
                content.layout(tree, renderer);

                let Content {
                    element,
                    layout: content_layout_node,
                    ..
                } = content.deref_mut();

                let content_layout = Layout::with_offset(
                    layout.bounds().position() - Point::ORIGIN,
                    content_layout_node.as_ref().unwrap(),
                );

                (
                    element
                        .as_widget_mut()
                        .overlay(tree, content_layout, renderer, translation)
                        .map(|overlay| RefCell::new(Nested::new(overlay))),
                    content_layout_node,
                )
            },
        }
        .build();

        if overlay.with_overlay(|(overlay, _layout)| overlay.is_some()) {
            Some(overlay::Element::new(Box::new(overlay)))
        } else {
            None
        }
    }

    #[cfg(feature = "a11y")]
    fn a11y_nodes(
        &self,
        layout: Layout<'_>,
        tree: &Tree,
        cursor_position: mouse::Cursor,
    ) -> iced_accessibility::A11yTree {
        use std::rc::Rc;

        let tree = tree.state.downcast_ref::<Rc<RefCell<Option<Tree>>>>();
        if let Some(tree) = tree.borrow().as_ref() {
            self.content.borrow().element.as_widget().a11y_nodes(
                layout,
                &tree.children[0],
                cursor_position,
            )
        } else {
            iced_accessibility::A11yTree::default()
        }
    }

    fn id(&self) -> Option<core::widget::Id> {
        self.content.borrow().element.as_widget().id()
    }

    fn set_id(&mut self, _id: iced_runtime::core::id::Id) {
        self.content
            .borrow_mut()
            .element
            .as_widget_mut()
            .set_id(_id);
    }

    fn drag_destinations(
        &self,
        state: &Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        dnd_rectangles: &mut core::clipboard::DndDestinationRectangles,
    ) {
        let ret = self.content.borrow_mut().resolve(
            &mut state.state.downcast_ref::<State>().tree.borrow_mut(),
            renderer,
            layout,
            &self.view,
            |tree, r, layout, element| {
                element.as_widget().drag_destinations(
                    tree,
                    layout,
                    r,
                    dnd_rectangles,
                );
            },
        );
        ret
    }
}

impl<'a, Message, Theme, Renderer>
    From<Responsive<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: core::Renderer + 'a,
{
    fn from(responsive: Responsive<'a, Message, Theme, Renderer>) -> Self {
        Self::new(responsive)
    }
}

#[self_referencing]
struct Overlay<'a, 'b, Message, Theme, Renderer> {
    content: RefMut<'a, Content<'b, Message, Theme, Renderer>>,
    tree: RefMut<'a, Tree>,
    types: PhantomData<Message>,

    #[borrows(mut content, mut tree)]
    #[not_covariant]
    overlay: (
        Option<RefCell<Nested<'this, Message, Theme, Renderer>>>,
        &'this mut Option<layout::Node>,
    ),
}

impl<'a, 'b, Message, Theme, Renderer>
    Overlay<'a, 'b, Message, Theme, Renderer>
{
    fn with_overlay_maybe<T>(
        &self,
        f: impl FnOnce(&mut Nested<'_, Message, Theme, Renderer>) -> T,
    ) -> Option<T> {
        self.with_overlay(|(overlay, _layout)| {
            overlay.as_ref().map(|nested| (f)(&mut nested.borrow_mut()))
        })
    }

    fn with_overlay_mut_maybe<T>(
        &mut self,
        f: impl FnOnce(&mut Nested<'_, Message, Theme, Renderer>) -> T,
    ) -> Option<T> {
        self.with_overlay_mut(|(overlay, _layout)| {
            overlay.as_mut().map(|nested| (f)(nested.get_mut()))
        })
    }
}

impl<'a, 'b, Message, Theme, Renderer>
    overlay::Overlay<Message, Theme, Renderer>
    for Overlay<'a, 'b, Message, Theme, Renderer>
where
    Renderer: core::Renderer,
{
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        self.with_overlay_maybe(|overlay| overlay.layout(renderer, bounds))
            .unwrap_or_default()
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        let _ = self.with_overlay_maybe(|overlay| {
            overlay.draw(renderer, theme, style, layout, cursor);
        });
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.with_overlay_maybe(|overlay| {
            overlay.mouse_interaction(layout, cursor, viewport, renderer)
        })
        .unwrap_or_default()
    }

    fn on_event(
        &mut self,
        event: Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) -> event::Status {
        let mut is_layout_invalid = false;

        let event_status = self
            .with_overlay_mut_maybe(|overlay| {
                let event_status = overlay.on_event(
                    event, layout, cursor, renderer, clipboard, shell,
                );

                is_layout_invalid = shell.is_layout_invalid();

                event_status
            })
            .unwrap_or(event::Status::Ignored);

        if is_layout_invalid {
            self.with_overlay_mut(|(_overlay, layout)| {
                **layout = None;
            });
        }

        event_status
    }

    fn is_over(
        &self,
        layout: Layout<'_>,
        renderer: &Renderer,
        cursor_position: Point,
    ) -> bool {
        self.with_overlay_maybe(|overlay| {
            overlay.is_over(layout, renderer, cursor_position)
        })
        .unwrap_or_default()
    }

    fn operate(
        &mut self,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let _ = self.with_overlay_mut_maybe(|overlay| {
            overlay.operate(layout, renderer, operation);
        });
    }
}
