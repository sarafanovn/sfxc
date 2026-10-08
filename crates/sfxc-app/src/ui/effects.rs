use eframe::egui;
use sfxc_core::patch::Effect;

pub fn show(ui: &mut egui::Ui, effects: &mut Vec<Effect>, next_id: u64) {
    let _ = (effects, next_id);
    ui.weak("Effects: coming in Task 13");
}
