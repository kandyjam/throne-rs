//! Native profile form; JSON and structured tabs share one lossless domain draft.
use gpui::{div, prelude::*, px, App, AppContext, Context, Entity, SharedString, Window};
use gpui_component::input::{InputState, TextareaState};
use throne_domain::{GroupId, ProfileEditDraft, ProfileEditKind, ProfileEditSection};

use super::dialog_inputs::DialogInputs;
use super::widgets::{
    dialog_actions, input_area_tall, input_field_row, line_input, section_hint,
    settings_switch_row, tab_bar,
};
use crate::theme::Theme;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProfileEditorTab {
    #[default]
    General,
    Transport,
    Tls,
    Json,
}
impl ProfileEditorTab {
    const ALL: [Self; 4] = [Self::General, Self::Transport, Self::Tls, Self::Json];
    pub fn from_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or_default()
    }
    fn index(self) -> usize {
        Self::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }
    fn section(self) -> Option<ProfileEditSection> {
        match self {
            Self::General => Some(ProfileEditSection::General),
            Self::Transport => Some(ProfileEditSection::Transport),
            Self::Tls => Some(ProfileEditSection::Tls),
            Self::Json => None,
        }
    }
}

#[derive(Clone)]
pub struct ProfileEditorInputs {
    pub name: Entity<InputState>,
    pub fields: Vec<Entity<InputState>>,
    pub json: Entity<TextareaState>,
}
impl ProfileEditorInputs {
    pub fn new<V: 'static>(
        draft: &ProfileEditDraft,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> Self {
        let name = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Profile name")
                .default_value(draft.name.clone())
        });
        let fields = draft
            .fields
            .iter()
            .map(|field| {
                cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder(field.label)
                        .default_value(field.value.clone())
                        .masked(field.kind == ProfileEditKind::Secret)
                })
            })
            .collect();
        let json = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(14)
                .placeholder("Outbound JSON object")
                .default_value(draft.json.clone())
        });
        Self { name, fields, json }
    }

    /// Read only the active representation; hidden JSON must never overwrite form edits.
    pub fn read_draft(
        &self,
        draft: &ProfileEditDraft,
        tab: ProfileEditorTab,
        cx: &App,
    ) -> Result<ProfileEditDraft, String> {
        let mut result = if tab == ProfileEditorTab::Json {
            draft.with_json(DialogInputs::read_string(&self.json, cx))?
        } else {
            let mut result = draft.clone();
            for (field, input) in result.fields.iter_mut().zip(&self.fields) {
                field.value = DialogInputs::read_string(input, cx);
            }
            result
        };
        result.name = DialogInputs::read_string(&self.name, cx);
        Ok(result)
    }
}

pub struct ProfileEditorView<'a> {
    pub inputs: &'a ProfileEditorInputs,
    pub draft: &'a ProfileEditDraft,
    pub tab: ProfileEditorTab,
    pub type_label: &'a str,
    pub error: &'a str,
    pub groups: &'a [(GroupId, String, bool)],
    pub is_selector: bool,
    pub content_height: f32,
}

pub enum ProfileEditorEvent {
    Tab(ProfileEditorTab),
    Toggle(usize),
    Group(GroupId),
    Save,
    Cancel,
}

pub fn profile_editor_body(
    view: ProfileEditorView<'_>,
    cx: &App,
    on_event: impl Fn(ProfileEditorEvent, &mut Window, &mut App) + Clone + 'static,
) -> impl IntoElement {
    let tabs_callback = on_event.clone();
    let tabs = tab_bar(
        "profile-editor-tabs",
        view.tab.index(),
        ["General", "Transport", "TLS / REALITY", "JSON"].map(SharedString::from),
        move |index, window, cx| {
            tabs_callback(
                ProfileEditorEvent::Tab(ProfileEditorTab::from_index(*index)),
                window,
                cx,
            )
        },
    );
    let mut content = div()
        .id("profile-editor-fields")
        .h(px(view.content_height))
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .gap_2();
    if view.tab == ProfileEditorTab::Json {
        content = content
            .child(section_hint(
                "Advanced outbound JSON · includes credentials. Switching tabs keeps your changes.",
            ))
            .child(input_area_tall(
                &view.inputs.json,
                (view.content_height - 48.).max(160.),
            ));
    } else {
        let section = view.tab.section();
        let mut count = 0;
        for (index, field) in view.draft.fields.iter().enumerate() {
            if Some(field.section) != section {
                continue;
            }
            count += 1;
            let input = &view.inputs.fields[index];
            if field.kind == ProfileEditKind::Boolean {
                let callback = on_event.clone();
                content = content.child(settings_switch_row(
                    format!("profile-field-{index}"),
                    field.label,
                    DialogInputs::read_string(input, cx) == "true",
                    move |_, w, cx| callback(ProfileEditorEvent::Toggle(index), w, cx),
                ));
            } else if field.kind == ProfileEditKind::Secret {
                content = content.child(
                    div()
                        .flex()
                        .gap_3()
                        .mb_2()
                        .w_full()
                        .child(
                            div()
                                .w(px(150.))
                                .flex_shrink_0()
                                .pt(px(6.))
                                .text_xs()
                                .text_color(Theme::text_muted())
                                .child(field.label),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .child(line_input(input).mask_toggle()),
                        ),
                );
            } else {
                content = content.child(input_field_row(field.label, input, 150.));
            }
        }
        if view.tab == ProfileEditorTab::General && view.is_selector {
            content = content.child(section_hint(
                "Tracked groups · selection order is preserved",
            ));
            for (id, label, selected) in view.groups {
                let id = *id;
                let callback = on_event.clone();
                content = content.child(settings_switch_row(
                    format!("selector-group-{id}"),
                    label.clone(),
                    *selected,
                    move |_, w, cx| callback(ProfileEditorEvent::Group(id), w, cx),
                ));
            }
        } else if count == 0 {
            content = content.child(section_hint(
                "Use the JSON tab to edit this profile's advanced configuration.",
            ));
        }
    }
    let save = on_event.clone();
    let cancel = on_event;
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(section_hint(format!("Type: {}", view.type_label)))
        .child(input_field_row("Name", &view.inputs.name, 150.))
        .child(tabs)
        .child(content)
        .when(!view.error.is_empty(), |body| {
            body.child(
                div()
                    .text_sm()
                    .text_color(Theme::danger())
                    .child(view.error.to_string()),
            )
        })
        .child(dialog_actions(
            "ep-cancel",
            "Cancel",
            "ep-ok",
            "Save",
            move |w, cx| cancel(ProfileEditorEvent::Cancel, w, cx),
            move |w, cx| save(ProfileEditorEvent::Save, w, cx),
        ))
}
