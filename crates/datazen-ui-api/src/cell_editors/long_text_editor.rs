use super::{text_area, TextAreaView};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LongTextEditorInput {
    pub auto_focus: bool,
    pub invalid: bool,
}

pub fn build_long_text_view(input: &LongTextEditorInput) -> TextAreaView {
    text_area(4, input.auto_focus, input.invalid)
}
