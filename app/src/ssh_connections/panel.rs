use warp_core::send_telemetry_from_ctx;
use warp_core::ui::Icon;
use warpui::elements::{
    resizable_state_handle, Align, Container, CornerRadius, CrossAxisAlignment,
    ConstrainedBox, DispatchEventResult, DragBarSide, Element, Empty, EventHandler, Flex,
    MouseStateHandle, ParentElement, Radius, Resizable, ResizableStateHandle, Shrinkable, Text,
};
use warpui::fonts::{Properties, Weight};
use warpui::platform::{Cursor, FilePickerConfiguration};
use warpui::presenter::ChildView;
use warpui::ui_components::components::UiComponent;

use warpui::{AppContext, Entity, SingletonEntity, TypedActionView, View, ViewContext, ViewHandle};

use crate::appearance::Appearance;
use crate::editor::{EditorOptions, EditorView, Event as EditorEvent, TextOptions};
use crate::root_view::SubshellCommandArg;
use crate::server::telemetry::TelemetryEvent;
use crate::ssh_connections::settings::{
    SshConnection, SshConnectionSettings, SshConnectionSettingsChangedEvent,
};
use crate::terminal::resizable_data::{ModalType, ResizableData, DEFAULT_RIGHT_PANEL_WIDTH};
use crate::ui_components::buttons::icon_button;
use crate::workspace::TAB_BAR_HEIGHT;
use settings::Setting as _;

const MIN_PANEL_WIDTH: f32 = 280.;
const MIN_REMAINING_WINDOW_SIZE: f32 = 200.;
const HEADER_HEIGHT: f32 = TAB_BAR_HEIGHT;
const PANEL_HORIZONTAL_PADDING: f32 = 12.;
const ROW_HORIZONTAL_PADDING: f32 = 8.;
const ROW_VERTICAL_PADDING: f32 = 6.;

pub enum SshConnectionsPanelEvent {
    ClosePanel,
    LaunchConnection(String), // id
}

#[derive(Debug, Clone)]
pub enum SshConnectionsPanelAction {
    ClosePanel,
    AddConnection,
    EditConnection(String),   // id
    DeleteConnection(String), // id
    LaunchConnection(String), // id
    SaveConnection,
    CancelEdit,
    UpdateLabel(String),
    UpdateHost(String),
    UpdatePort(String),
    UpdateUser(String),
    UpdateIdentityFile(Option<String>),
    OpenIdentityFilePicker,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PanelMode {
    List,
    Add,
    Edit(String), // id
}

pub struct SshConnectionsPanelView {
    mode: PanelMode,
    connections: Vec<SshConnection>,
    // Form fields (for add/edit)
    form_label: String,
    form_host: String,
    form_port: String,
    form_user: String,
    form_identity_file: Option<String>,
    // Interactive editors for form fields
    label_editor: ViewHandle<EditorView>,
    host_editor: ViewHandle<EditorView>,
    port_editor: ViewHandle<EditorView>,
    user_editor: ViewHandle<EditorView>,
    // Child views
    resizable_state_handle: ResizableStateHandle,
    close_button_mouse_state: MouseStateHandle,
    add_button_mouse_state: MouseStateHandle,
}

impl SshConnectionsPanelView {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let settings = SshConnectionSettings::handle(ctx);
        let connections = settings.as_ref(ctx).connections.value().clone();

        let resizable_data_handle = ResizableData::handle(ctx);
        let resizable_state_handle = match resizable_data_handle
            .as_ref(ctx)
            .get_handle(ctx.window_id(), ModalType::RightPanelWidth)
        {
            Some(handle) => handle,
            None => {
                log::error!("Couldn't retrieve SSH connections panel resizable state handle.");
                resizable_state_handle(DEFAULT_RIGHT_PANEL_WIDTH)
            }
        };

        let label_editor = Self::create_text_editor(ctx);
        let host_editor = Self::create_text_editor(ctx);
        let port_editor = Self::create_text_editor(ctx);
        let user_editor = Self::create_text_editor(ctx);

        ctx.subscribe_to_view(&label_editor, |me, _, event, ctx| {
            if let EditorEvent::Edited(_) = event {
                let text = me.label_editor.as_ref(ctx).buffer_text(ctx);
                me.form_label = text;
            }
        });
        ctx.subscribe_to_view(&host_editor, |me, _, event, ctx| {
            if let EditorEvent::Edited(_) = event {
                let text = me.host_editor.as_ref(ctx).buffer_text(ctx);
                me.form_host = text;
            }
        });
        ctx.subscribe_to_view(&port_editor, |me, _, event, ctx| {
            if let EditorEvent::Edited(_) = event {
                let text = me.port_editor.as_ref(ctx).buffer_text(ctx);
                me.form_port = text;
            }
        });
        ctx.subscribe_to_view(&user_editor, |me, _, event, ctx| {
            if let EditorEvent::Edited(_) = event {
                let text = me.user_editor.as_ref(ctx).buffer_text(ctx);
                me.form_user = text;
            }
        });

        ctx.subscribe_to_model(&settings, |me, _, event, ctx| {
            let SshConnectionSettingsChangedEvent::Connections { .. } = event;
            let settings = SshConnectionSettings::handle(ctx);
            me.connections = settings.as_ref(ctx).connections.value().clone();
            ctx.notify();
        });

        Self {
            mode: PanelMode::List,
            connections,
            form_label: String::new(),
            form_host: String::new(),
            form_port: "22".to_string(),
            form_user: String::new(),
            form_identity_file: None,
            label_editor,
            host_editor,
            port_editor,
            user_editor,
            resizable_state_handle,
            close_button_mouse_state: Default::default(),
            add_button_mouse_state: Default::default(),
        }
    }

    fn create_text_editor(ctx: &mut ViewContext<Self>) -> ViewHandle<EditorView> {
        let appearance = Appearance::as_ref(ctx);
        let options = EditorOptions {
            text: TextOptions::ui_text(Some(13.), appearance),
            autogrow: true,
            soft_wrap: false,
            ..Default::default()
        };
        ctx.add_typed_action_view(|ctx| EditorView::new(options, ctx))
    }

    fn set_form_from_connection(&mut self, conn: &SshConnection) {
        self.form_label = conn.label.clone();
        self.form_host = conn.host.clone();
        self.form_port = conn.port.to_string();
        self.form_user = conn.user.clone();
        self.form_identity_file = conn.identity_file.clone();
    }

    fn clear_form(&mut self) {
        self.form_label.clear();
        self.form_host.clear();
        self.form_port = "22".to_string();
        self.form_user.clear();
        self.form_identity_file = None;
    }

    fn sync_editors_from_form(&self, ctx: &mut ViewContext<Self>) {
        self.label_editor
            .update(ctx, |ed, ctx| ed.set_buffer_text(&self.form_label, ctx));
        self.host_editor
            .update(ctx, |ed, ctx| ed.set_buffer_text(&self.form_host, ctx));
        self.port_editor
            .update(ctx, |ed, ctx| ed.set_buffer_text(&self.form_port, ctx));
        self.user_editor
            .update(ctx, |ed, ctx| ed.set_buffer_text(&self.form_user, ctx));
    }

    fn clear_editors(&self, ctx: &mut ViewContext<Self>) {
        self.label_editor.update(ctx, |ed, ctx| ed.clear_buffer(ctx));
        self.host_editor.update(ctx, |ed, ctx| ed.clear_buffer(ctx));
        self.port_editor
            .update(ctx, |ed, ctx| ed.set_buffer_text("22", ctx));
        self.user_editor.update(ctx, |ed, ctx| ed.clear_buffer(ctx));
    }

    fn save_current_form(&mut self, ctx: &mut ViewContext<Self>) {
        let port = self.form_port.parse::<u16>().unwrap_or(22);
        let id = match &self.mode {
            PanelMode::Edit(id) => id.clone(),
            _ => uuid::Uuid::new_v4().to_string(),
        };

        let conn = SshConnection {
            id,
            label: self.form_label.trim().to_string(),
            host: self.form_host.trim().to_string(),
            port,
            user: self.form_user.trim().to_string(),
            identity_file: self.form_identity_file.clone(),
        };

        let settings = SshConnectionSettings::handle(ctx);
        settings.update(ctx, |settings, ctx| {
            let mut new_connections: Vec<SshConnection> = settings
                .connections
                .value()
                .iter()
                .filter(|c| c.id != conn.id)
                .cloned()
                .collect();
            new_connections.push(conn.clone());
            settings.connections.set_value(new_connections, ctx).unwrap();
        });

        send_telemetry_from_ctx!(
            match &self.mode {
                PanelMode::Edit(_) => TelemetryEvent::SshConnectionEdited,
                _ => TelemetryEvent::SshConnectionCreated,
            },
            ctx
        );

        self.mode = PanelMode::List;
        self.clear_form();
        self.clear_editors(ctx);
    }

    fn delete_connection(&mut self, id: &str, ctx: &mut ViewContext<Self>) {
        let settings = SshConnectionSettings::handle(ctx);
        settings.update(ctx, |settings, ctx| {
            let new_connections: Vec<SshConnection> = settings
                .connections
                .value()
                .iter()
                .filter(|c| c.id != id)
                .cloned()
                .collect();
            settings.connections.set_value(new_connections, ctx).unwrap();
        });

        send_telemetry_from_ctx!(TelemetryEvent::SshConnectionDeleted, ctx);
    }

    fn launch_connection(&mut self, id: &str, ctx: &mut ViewContext<Self>) {
        if let Some(conn) = self.connections.iter().find(|c| c.id == id) {
            let command = conn.to_command_string();
            let arg = SubshellCommandArg {
                command,
                shell_type: None,
            };
            ctx.dispatch_global_action(
                "root_view:open_new_tab_insert_subshell_command_and_bootstrap_if_supported",
                arg,
            );
            send_telemetry_from_ctx!(TelemetryEvent::SshConnectionLaunched, ctx);
        }
    }
}

impl Entity for SshConnectionsPanelView {
    type Event = SshConnectionsPanelEvent;
}

impl TypedActionView for SshConnectionsPanelView {
    type Action = SshConnectionsPanelAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            SshConnectionsPanelAction::ClosePanel => {
                ctx.emit(SshConnectionsPanelEvent::ClosePanel);
            }
            SshConnectionsPanelAction::AddConnection => {
                self.clear_form();
                self.clear_editors(ctx);
                self.mode = PanelMode::Add;
                ctx.notify();
            }
            SshConnectionsPanelAction::EditConnection(id) => {
                let conn = self.connections.iter().find(|c| c.id == *id).cloned();
                if let Some(conn) = conn {
                    self.set_form_from_connection(&conn);
                    self.sync_editors_from_form(ctx);
                    self.mode = PanelMode::Edit(id.clone());
                    ctx.notify();
                }
            }
            SshConnectionsPanelAction::DeleteConnection(id) => {
                self.delete_connection(id, ctx);
            }
            SshConnectionsPanelAction::LaunchConnection(id) => {
                self.launch_connection(id, ctx);
            }
            SshConnectionsPanelAction::SaveConnection => {
                self.save_current_form(ctx);
                ctx.notify();
            }
            SshConnectionsPanelAction::CancelEdit => {
                self.mode = PanelMode::List;
                self.clear_form();
                self.clear_editors(ctx);
                ctx.notify();
            }
            SshConnectionsPanelAction::UpdateLabel(text) => {
                self.form_label = text.clone();
            }
            SshConnectionsPanelAction::UpdateHost(text) => {
                self.form_host = text.clone();
            }
            SshConnectionsPanelAction::UpdatePort(text) => {
                self.form_port = text.clone();
            }
            SshConnectionsPanelAction::UpdateUser(text) => {
                self.form_user = text.clone();
            }
            SshConnectionsPanelAction::UpdateIdentityFile(path) => {
                self.form_identity_file = path.clone();
                ctx.notify();
            }
            SshConnectionsPanelAction::OpenIdentityFilePicker => {
                ctx.open_file_picker(
                    |result, ctx| {
                        if let Ok(paths) = result {
                            if let Some(path) = paths.into_iter().next() {
                                ctx.dispatch_typed_action_deferred(
                                    SshConnectionsPanelAction::UpdateIdentityFile(Some(path)),
                                );
                            }
                        }
                    },
                    FilePickerConfiguration::new(),
                );
            }
        }
    }
}

impl View for SshConnectionsPanelView {
    fn ui_name() -> &'static str {
        "SshConnectionsPanel"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();

        let header = self.render_header(app);
        let body = match &self.mode {
            PanelMode::List => self.render_list(app),
            PanelMode::Add | PanelMode::Edit(_) => self.render_form(app),
        };

        let content = Flex::column()
            .with_child(
                ConstrainedBox::new(
                    Container::new(header)
                        .with_padding_left(PANEL_HORIZONTAL_PADDING)
                        .with_padding_right(PANEL_HORIZONTAL_PADDING)
                        .finish(),
                )
                .with_height(HEADER_HEIGHT)
                .finish(),
            )
            .with_child(
                Container::new(body)
                    .with_padding_left(PANEL_HORIZONTAL_PADDING)
                    .with_padding_right(PANEL_HORIZONTAL_PADDING)
                    .with_padding_bottom(PANEL_HORIZONTAL_PADDING)
                    .finish(),
            )
            .finish();

        let clickable_panel = EventHandler::new(content).on_left_mouse_down(|_, _, _| {
            // Prevent clicks from propagating to underlying workspace
            DispatchEventResult::StopPropagation
        });

        Resizable::new(
            self.resizable_state_handle.clone(),
            Container::new(clickable_panel.finish())
                .with_background(theme.background())
                .finish(),
        )
        .with_dragbar_side(DragBarSide::Left)
        .with_bounds_callback(Box::new(|window_bounds| {
            (
                MIN_PANEL_WIDTH,
                (window_bounds.x() - MIN_REMAINING_WINDOW_SIZE).max(MIN_PANEL_WIDTH),
            )
        }))
        .on_resize(|ctx, _| ctx.notify())
        .finish()
    }
}

// Rendering helpers
impl SshConnectionsPanelView {
    fn render_header(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();
        let ui_font = appearance.ui_font_family();

        let title = Text::new_inline("SSH Connections", ui_font, 14.)
            .with_style(Properties::default().weight(Weight::Semibold))
            .with_color(theme.foreground().into_solid())
            .finish();

        let add_button = icon_button(
            appearance,
            Icon::Plus,
            false,
            self.add_button_mouse_state.clone(),
        )
        .build()
        .on_click(|ctx, _, _| {
            ctx.dispatch_typed_action(SshConnectionsPanelAction::AddConnection);
        })
        .with_cursor(Cursor::PointingHand)
        .finish();

        let close_button = icon_button(
            appearance,
            Icon::X,
            false,
            self.close_button_mouse_state.clone(),
        )
        .build()
        .on_click(|ctx, _, _| {
            ctx.dispatch_typed_action(SshConnectionsPanelAction::ClosePanel);
        })
        .with_cursor(Cursor::PointingHand)
        .finish();

        Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(title)
            .with_child(Shrinkable::new(1., Empty::new().finish()).finish())
            .with_child(add_button)
            .with_child(close_button)
            .finish()
    }

    fn render_list(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();
        let ui_font = appearance.ui_font_family();

        if self.connections.is_empty() {
            return Align::new(
                Text::new_inline("No saved connections. Click + to add one.", ui_font, 13.)
                    .with_color(theme.foreground().into_solid())
                    .finish(),
            )
            .finish();
        }

        let mut column = Flex::column();

        for conn in &self.connections {
            let conn_id = conn.id.clone();
            let label = if conn.label.is_empty() {
                conn.host.clone()
            } else {
                conn.label.clone()
            };
            let subtitle = format!(
                "{}@{}:{}",
                if conn.user.is_empty() { "root" } else { &conn.user },
                conn.host,
                conn.port
            );

            let connect_button = icon_button(
                appearance,
                Icon::Terminal,
                false,
                MouseStateHandle::default(),
            )
            .build()
            .on_click({
                let id = conn_id.clone();
                move |ctx, _, _| {
                    ctx.dispatch_typed_action(SshConnectionsPanelAction::LaunchConnection(
                        id.clone(),
                    ));
                }
            })
            .with_cursor(Cursor::PointingHand)
            .finish();

            let edit_button = icon_button(
                appearance,
                Icon::Pencil,
                false,
                MouseStateHandle::default(),
            )
            .build()
            .on_click({
                let id = conn_id.clone();
                move |ctx, _, _| {
                    ctx.dispatch_typed_action(SshConnectionsPanelAction::EditConnection(
                        id.clone(),
                    ));
                }
            })
            .with_cursor(Cursor::PointingHand)
            .finish();

            let delete_button = icon_button(
                appearance,
                Icon::Trash,
                false,
                MouseStateHandle::default(),
            )
            .build()
            .on_click({
                let id = conn_id.clone();
                move |ctx, _, _| {
                    ctx.dispatch_typed_action(SshConnectionsPanelAction::DeleteConnection(
                        id.clone(),
                    ));
                }
            })
            .with_cursor(Cursor::PointingHand)
            .finish();

            let text_column = Flex::column()
                .with_child(
                    Text::new_inline(label.clone(), ui_font, 13.)
                        .with_color(theme.foreground().into_solid())
                        .finish(),
                )
                .with_child(
                    Text::new_inline(subtitle.clone(), ui_font, 11.)
                        .with_color(theme.foreground().into_solid())
                        .finish(),
                )
                .with_cross_axis_alignment(CrossAxisAlignment::Start)
                .finish();

            let row_content = Flex::row()
                .with_child(text_column)
                .with_child(Shrinkable::new(1., Empty::new().finish()).finish())
                .with_child(connect_button)
                .with_child(edit_button)
                .with_child(delete_button)
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .finish();

            let row = Container::new(row_content)
                .with_padding_left(ROW_HORIZONTAL_PADDING)
                .with_padding_right(ROW_HORIZONTAL_PADDING)
                .with_padding_top(ROW_VERTICAL_PADDING)
                .with_padding_bottom(ROW_VERTICAL_PADDING)
                .with_background(theme.surface_1())
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(6.)))
                .finish();

            column.add_child(row);
        }

        column
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .finish()
    }

    fn render_form(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();
        let ui_font = appearance.ui_font_family();

        let mut column = Flex::column();

        // Helper to render a label + editor pair
        let render_field =
            |label_str: String, editor_handle: &ViewHandle<EditorView>| -> Box<dyn Element> {
                let label_text = Text::new_inline(label_str, ui_font, 12.)
                    .with_color(theme.foreground().into_solid())
                    .finish();

                let input = Container::new(ChildView::new(editor_handle).finish())
                    .with_padding_left(8.)
                    .with_padding_right(8.)
                    .with_padding_top(6.)
                    .with_padding_bottom(6.)
                    .with_background(theme.surface_1())
                    .with_corner_radius(CornerRadius::with_all(Radius::Pixels(6.)))
                    .finish();

                Container::new(
                    Flex::column()
                        .with_child(label_text)
                        .with_child(input)
                        .with_cross_axis_alignment(CrossAxisAlignment::Start)
                        .finish(),
                )
                .with_padding_bottom(12.)
                .finish()
            };

        column.add_child(render_field("Label".to_string(), &self.label_editor));
        column.add_child(render_field("Host".to_string(), &self.host_editor));
        column.add_child(render_field("Port".to_string(), &self.port_editor));
        column.add_child(render_field("User".to_string(), &self.user_editor));

        // Identity file
        let identity_label = Text::new_inline("Identity file", ui_font, 12.)
            .with_color(theme.foreground().into_solid())
            .finish();

        let identity_value: String = self
            .form_identity_file
            .clone()
            .unwrap_or_else(|| "None selected".to_string());
        let identity_text = Container::new(
            Text::new_inline(identity_value, ui_font, 13.)
                .with_color(theme.foreground().into_solid())
                .finish(),
        )
        .with_padding_left(8.)
        .with_padding_right(8.)
        .with_padding_top(6.)
        .with_padding_bottom(6.)
        .with_background(theme.surface_1())
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(6.)))
        .finish();

        let browse_button = EventHandler::new(
            Text::new_inline("Browse...", ui_font, 12.)
                .with_color(theme.accent().into_solid())
                .finish(),
        )
        .on_left_mouse_down(|ctx, _, _| {
            ctx.dispatch_typed_action(SshConnectionsPanelAction::OpenIdentityFilePicker);
            DispatchEventResult::StopPropagation
        })
        .finish();

        let identity_row = Flex::row()
            .with_child(identity_text)
            .with_child(Shrinkable::new(1., Empty::new().finish()).finish())
            .with_child(browse_button)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .finish();

        column.add_child(identity_label);
        column.add_child(identity_row);
        column.add_child(
            Container::new(Empty::new().finish())
                .with_padding_bottom(12.)
                .finish(),
        );

        // Buttons
        let save_label = Text::new_inline("Save", ui_font, 13.)
            .with_color(theme.background().into_solid())
            .finish();
        let save_button = EventHandler::new(
            Container::new(save_label)
                .with_padding_left(16.)
                .with_padding_right(16.)
                .with_padding_top(8.)
                .with_padding_bottom(8.)
                .with_background(theme.accent())
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(6.)))
                .finish(),
        )
        .on_left_mouse_down(|ctx, _, _| {
            ctx.dispatch_typed_action(SshConnectionsPanelAction::SaveConnection);
            DispatchEventResult::StopPropagation
        })
        .finish();

        let cancel_label = Text::new_inline("Cancel", ui_font, 13.)
            .with_color(theme.foreground().into_solid())
            .finish();
        let cancel_button = EventHandler::new(
            Container::new(cancel_label)
                .with_padding_left(16.)
                .with_padding_right(16.)
                .with_padding_top(8.)
                .with_padding_bottom(8.)
                .with_background(theme.surface_1())
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(6.)))
                .finish(),
        )
        .on_left_mouse_down(|ctx, _, _| {
            ctx.dispatch_typed_action(SshConnectionsPanelAction::CancelEdit);
            DispatchEventResult::StopPropagation
        })
        .finish();

        let buttons = Flex::row()
            .with_child(save_button)
            .with_child(cancel_button)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .finish();

        column.add_child(buttons);

        column
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .finish()
    }
}
