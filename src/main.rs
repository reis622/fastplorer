//! Fastplorer — PowerToys Run benzeri dosya başlatıcı (Windows GUI, v0.2).
#![cfg_attr(windows, windows_subsystem = "windows")]

#[macro_use]
mod i18n;
mod app;
mod bin;
mod config;
mod index;
mod notes;
mod ops;
mod platform;
mod preview;
mod recent;
mod recycle;
mod search;
mod selection;
mod startup;
mod terminal;
mod theme;
mod util;
mod view;

use eframe::egui;
use std::sync::Arc;

struct Launcher {
    app: app::App,
}

impl eframe::App for Launcher {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if std::env::var_os("BUL_DEBUG").is_some() {
            let n = ctx.input(|i| i.events.len());
            eprintln!("kare {:?} olay={n} odak={:?}", std::time::SystemTime::now(), ctx.input(|i| i.viewport().focused));
        }
        self.app.frame_start(&ctx);
        view::draw(&mut self.app, ui);
        self.app.frame_end(&ctx);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // Saydamlık yok: pencere = kutu; kenarlarda siyah görünmesin diye panel rengiyle temizle.
        egui::Rgba::from(theme::PANEL_BOTTOM).to_array()
    }

    fn persist_egui_memory(&self) -> bool {
        false
    }
}

fn read_font(cands: &[std::path::PathBuf]) -> Option<Vec<u8>> {
    cands.iter().find_map(|p| std::fs::read(p).ok())
}

/// Segoe UI (Windows) ya da Selawik/ortam değişkeni; bulunamazsa egui'nin gömülü yazı tipleri (Türkçe harfleri içerir).
fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let mut regs: Vec<std::path::PathBuf> = Vec::new();
    let mut semis: Vec<std::path::PathBuf> = Vec::new();
    let mut monos: Vec<std::path::PathBuf> = Vec::new();
    if let Some(p) = std::env::var_os("BUL_FONT") {
        regs.push(p.into());
    }
    if let Some(p) = std::env::var_os("BUL_FONT_SEMI") {
        semis.push(p.into());
    }
    let win = std::path::PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into())).join("Fonts");
    regs.push(win.join("segoeui.ttf"));
    semis.push(win.join("seguisb.ttf"));
    semis.push(win.join("segoeuib.ttf"));
    monos.push(win.join("CascadiaMono.ttf"));
    monos.push(win.join("consola.ttf"));
    let sel = std::path::PathBuf::from("/usr/share/fonts/truetype/sand-box/custom/Selawik");
    regs.push(sel.join("selawk.ttf"));
    semis.push(sel.join("selawksb.ttf"));
    monos.push("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf".into());

    let fallback: Vec<String> = fonts.families.get(&egui::FontFamily::Proportional).cloned().unwrap_or_default();
    let reg = read_font(&regs);
    let semi = read_font(&semis);
    if let Some(r) = reg {
        fonts.font_data.insert("ui".into(), Arc::new(egui::FontData::from_owned(r)));
        fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "ui".into());
    }
    let mut semi_fam: Vec<String> = Vec::new();
    if let Some(s) = semi {
        fonts.font_data.insert("ui-semi".into(), Arc::new(egui::FontData::from_owned(s)));
        semi_fam.push("ui-semi".into());
    } else if fonts.font_data.contains_key("ui") {
        semi_fam.push("ui".into());
    }
    semi_fam.extend(fallback);
    fonts.families.insert(view::semi(), semi_fam);
    if let Some(m) = read_font(&monos) {
        fonts.font_data.insert("mono".into(), Arc::new(egui::FontData::from_owned(m)));
        fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().insert(0, "mono".into());
    }
    ctx.set_fonts(fonts);
}

fn style(ctx: &egui::Context) {
    ctx.all_styles_mut(|s| {
        let v = &mut s.visuals;
        *v = egui::Visuals::dark();
        v.text_cursor.stroke = egui::Stroke::new(1.5, egui::Color32::from_rgb(0xf3, 0xf4, 0xf6));
        v.text_cursor.blink = true;
        v.text_cursor.on_duration = 0.53;
        v.text_cursor.off_duration = 0.53;
        v.selection.bg_fill = egui::Color32::from_rgb(0x2f, 0x4a, 0x66);
        v.selection.stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
        v.extreme_bg_color = egui::Color32::TRANSPARENT;
        v.window_fill = egui::Color32::TRANSPARENT;
        v.panel_fill = egui::Color32::TRANSPARENT;
        s.interaction.selectable_labels = false;
    });
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let hidden = args.iter().any(|a| a == "--hidden" || a == "--arka-plan");
    let retry = startup::retry_arg(&args);
    // çalışma klasörüne güvenme (Windows açılışında System32 olur), açılış günlüğü, panik kaydı
    startup::prepare(&args);
    if retry > 0 {
        std::thread::sleep(startup::retry_delay(retry));
    }
    // İkinci örnek: ilkine "göster" der ve sessizce çıkar (--hidden ile açılışta göstermez).
    if !platform::single_instance(!hidden, retry > 0) {
        return Ok(());
    }
    // eski "bul" verilerini (ilk açılışta bir kez) Fastplorer klasörüne taşı
    bin::migrate_legacy();
    let (config, first_run) = config::Config::load();
    i18n::set(config.ui_lang());
    if first_run {
        config.save();
    }
    if config.autostart {
        // ilk çalıştırmada varsayılan açık; her açılışta yolu tazele (exe taşınmış olabilir)
        let _ = platform::set_autostart(true, false);
    }
    // F9 ve tepsi simgesi arayüzden bağımsız başlar: --hidden ile pencere hiç çizilmese de çalışır
    platform::start_background(config.hotkey);
    let w = app::PANEL_W;
    let h = 400.0;
    let viewport = egui::ViewportBuilder::default()
        .with_title(platform::WINDOW_TITLE)
        .with_decorations(false)
        .with_transparent(false)
        .with_resizable(false)
        .with_always_on_top()
        .with_taskbar(false)
        .with_has_shadow(false)
        .with_visible(!hidden)
        .with_inner_size([w, h]);
    let options = eframe::NativeOptions {
        viewport,
        clipboard_shortcuts: false,
        centered: true,
        persist_window: false,
        ..Default::default()
    };
    // Windows açılışında ekran sürücüsü henüz hazır değilse pencere/OpenGL oluşturma geçici olarak
    // başarısız olabilir: hata ya da panikte günlüğe yaz ve biraz sonra kendini yeniden başlat.
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        eframe::run_native(
            "Fastplorer",
            options,
            Box::new(move |cc| {
                platform::set_ctx(&cc.egui_ctx);
                setup_fonts(&cc.egui_ctx);
                style(&cc.egui_ctx);
                startup::log("arayüz hazır");
                Ok(Box::new(Launcher { app: app::App::new(hidden, config) }))
            }),
        )
    }));
    let failed = match &r {
        Ok(Ok(())) => None,
        Ok(Err(e)) => Some(e.to_string()),
        Err(_) => Some("beklenmeyen hata (panik)".to_string()),
    };
    match failed {
        None => Ok(()),
        Some(e) => {
            startup::log(format!("arayüz başlatılamadı: {e}"));
            platform::shutdown();
            if retry < startup::MAX_RETRY && startup::relaunch(hidden, retry + 1) {
                return Ok(());
            }
            match r {
                Ok(res) => res,
                Err(p) => std::panic::resume_unwind(p),
            }
        }
    }
}
