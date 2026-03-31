use eframe::egui;

#[derive(Default)]
pub struct LiveCaptionRs {
    speech_to_text: String,
}

impl LiveCaptionRs {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self::default()
    }
}

impl eframe::App for LiveCaptionRs {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.label(&self.speech_to_text);
        });
    }
}
