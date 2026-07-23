#[derive(Debug, Clone, Default, PartialEq)]
pub struct CursorSamplePlaceholder {
    pub screen_x: f32,
    pub screen_y: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DwellStatePlaceholder {
    pub is_dwelling: bool,
}
