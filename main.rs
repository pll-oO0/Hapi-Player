#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use anyhow::{anyhow, Context, Result};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig,
};
use eframe::egui::{
    self, Align2, Button, CentralPanel, Color32, FontData, FontDefinitions, FontFamily, FontId,
    Frame, Margin, Pos2, Rect, RichText, Rounding, Sense, Stroke, TopBottomPanel, Vec2,
    ViewportBuilder,
};
use encoding_rs::{Encoding, GB18030};
use symphonia::core::{
    audio::{AudioBufferRef, Signal},
    codecs::DecoderOptions,
    conv::FromSample as SymphoniaFromSample,
    formats::FormatOptions,
    io::MediaSourceStream,
    meta::MetadataOptions,
    probe::Hint,
    sample::Sample as SymphoniaSample,
};

const DEFAULT_WINDOW_WIDTH: f32 = 1040.0;
const DEFAULT_WINDOW_HEIGHT: f32 = 760.0;
const WAVEFORM_BUCKETS: usize = 1800;

fn main() -> eframe::Result<()> {
    let app_title = "Hapi Player";
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT])
            .with_min_inner_size([820.0, 600.0])
            .with_title(app_title)
            .with_drag_and_drop(true),
        ..Default::default()
    };

    eframe::run_native(
        app_title,
        options,
        Box::new(|cc| Ok(Box::new(KaraokeApp::new(cc)))),
    )
}

struct KaraokeApp {
    lyrics: Vec<LyricLine>,
    lrc_path: Option<PathBuf>,
    tracks: [Option<Track>; 2],
    player: Option<AudioPlayer>,
    playing: bool,
    error: Option<String>,
    next_replace_slot: usize,
    language: Language,
    lyric_drag_start: Option<usize>,
    lyric_drag_preview: Option<usize>,
    calibration_seconds: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Language {
    Chinese,
    English,
}

struct UiText {
    app_title: &'static str,
    play: &'static str,
    pause: &'static str,
    stop: &'static str,
    choose_lyrics: &'static str,
    choose_audio: &'static str,
    lyric_filter: &'static str,
    lyric_card: &'static str,
    audio_track: &'static str,
    click_or_drop: &'static str,
    waiting_audio: &'static str,
    waiting_lyrics: &'static str,
    drop_to_import: &'static str,
    no_audio: &'static str,
    lrc_error: &'static str,
    audio_import_error: &'static str,
    audio_device_error: &'static str,
    calibrate: &'static str,
    save_lyrics_as: &'static str,
    seconds_unit: &'static str,
    export_error: &'static str,
    export_success: &'static str,
}

impl Language {
    fn text(self) -> UiText {
        match self {
            Language::Chinese => UiText {
                app_title: "Hapi Player",
                play: "\u{64ad}\u{653e}",
                pause: "\u{6682}\u{505c}",
                stop: "\u{505c}\u{6b62}",
                choose_lyrics: "\u{9009}\u{62e9}\u{6b4c}\u{8bcd}",
                choose_audio: "\u{9009}\u{62e9}\u{97f3}\u{9891}",
                lyric_filter: "\u{6b4c}\u{8bcd}\u{6587}\u{672c}",
                lyric_card: "\u{6b4c}\u{8bcd} LRC/TXT",
                audio_track: "\u{97f3}\u{9891}\u{8f68}",
                click_or_drop: "\u{70b9}\u{51fb}\u{6216}\u{62d6}\u{5165}\u{6587}\u{4ef6}",
                waiting_audio: "\u{7b49}\u{5f85}\u{97f3}\u{9891}",
                waiting_lyrics: "\u{7b49}\u{5f85}\u{6b4c}\u{8bcd}",
                drop_to_import: "\u{677e}\u{5f00}\u{5373}\u{53ef}\u{5bfc}\u{5165}\u{6587}\u{4ef6}",
                no_audio: "\u{8bf7}\u{5148}\u{5bfc}\u{5165}\u{81f3}\u{5c11}\u{4e00}\u{8f68}\u{97f3}\u{9891}\u{3002}",
                lrc_error: "LRC \u{89e3}\u{6790}\u{5931}\u{8d25}",
                audio_import_error: "\u{97f3}\u{9891}\u{5bfc}\u{5165}\u{5931}\u{8d25}",
                audio_device_error: "\u{97f3}\u{9891}\u{8bbe}\u{5907}\u{521d}\u{59cb}\u{5316}\u{5931}\u{8d25}",
                calibrate: "\u{6b4c}\u{8bcd}\u{6821}\u{51c6}",
                save_lyrics_as: "\u{6b4c}\u{8bcd}\u{53e6}\u{5b58}\u{4e3a}",
                seconds_unit: "S",
                export_error: "\u{6b4c}\u{8bcd}\u{5bfc}\u{51fa}\u{5931}\u{8d25}",
                export_success: "\u{6b4c}\u{8bcd}\u{5df2}\u{5bfc}\u{51fa}",
            },
            Language::English => UiText {
                app_title: "Hapi Player",
                play: "Play",
                pause: "Pause",
                stop: "Stop",
                choose_lyrics: "Choose Lyrics",
                choose_audio: "Choose Audio",
                lyric_filter: "Lyrics Text",
                lyric_card: "Lyrics LRC/TXT",
                audio_track: "Audio Track",
                click_or_drop: "Click or drop a file",
                waiting_audio: "Waiting for audio",
                waiting_lyrics: "Waiting for lyrics",
                drop_to_import: "Release to import files",
                no_audio: "Please import at least one audio track first.",
                lrc_error: "Failed to parse lyrics",
                audio_import_error: "Failed to import audio",
                audio_device_error: "Failed to initialize audio device",
                calibrate: "Calibrate Lyrics",
                save_lyrics_as: "Save Lyrics As",
                seconds_unit: "S",
                export_error: "Failed to export lyrics",
                export_success: "Lyrics exported",
            },
        }
    }
}

impl KaraokeApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_chinese_fonts(&cc.egui_ctx);
        cc.egui_ctx.set_pixels_per_point(1.1);
        Self {
            lyrics: Vec::new(),
            lrc_path: None,
            tracks: [None, None],
            player: None,
            playing: false,
            error: None,
            next_replace_slot: 0,
            language: Language::Chinese,
            lyric_drag_start: None,
            lyric_drag_preview: None,
            calibration_seconds: "0".to_owned(),
        }
    }

    fn load_lrc(&mut self, path: PathBuf) {
        match parse_lrc_file(&path) {
            Ok(lyrics) => {
                self.lyrics = lyrics;
                self.lrc_path = Some(path);
                self.error = None;
            }
            Err(err) => {
                let text = self.language.text();
                self.error = Some(format!("{}: {err:#}", text.lrc_error));
            }
        }
    }

    fn load_track(&mut self, index: usize, path: PathBuf) {
        match decode_track(&path) {
            Ok(track) => {
                self.tracks[index] = Some(track);
                self.next_replace_slot = (index + 1) % 2;
                self.rebuild_player();
                self.error = None;
            }
            Err(err) => {
                let text = self.language.text();
                self.error = Some(format!("{}: {err:#}", text.audio_import_error));
            }
        }
    }

    fn load_dropped_files(&mut self, paths: Vec<PathBuf>) {
        let mut audio_paths = Vec::new();

        for path in paths {
            if is_lyric_file(&path) {
                self.load_lrc(path);
            } else {
                audio_paths.push(path);
            }
        }

        for path in audio_paths {
            let index = self.next_audio_slot();
            self.load_track(index, path);
        }
    }

    fn next_audio_slot(&self) -> usize {
        if self.tracks[0].is_none() {
            0
        } else if self.tracks[1].is_none() {
            1
        } else {
            self.next_replace_slot
        }
    }

    fn rebuild_player(&mut self) {
        self.playing = false;
        self.player = None;

        let Some(track_a) = &self.tracks[0] else {
            return;
        };

        let tracks = [
            track_a.audio.clone(),
            self.tracks[1]
                .as_ref()
                .map(|track| track.audio.clone())
                .unwrap_or_else(|| Arc::new(AudioData::silence_like(track_a))),
        ];

        match AudioPlayer::new(tracks) {
            Ok(player) => self.player = Some(player),
            Err(err) => {
                let text = self.language.text();
                self.error = Some(format!("{}: {err:#}", text.audio_device_error));
            }
        }
    }

    fn current_time(&self) -> f32 {
        self.player
            .as_ref()
            .map(AudioPlayer::current_seconds)
            .unwrap_or_default()
    }

    fn toggle_play(&mut self) {
        let Some(player) = &self.player else {
            self.error = Some(self.language.text().no_audio.to_owned());
            return;
        };

        self.playing = !self.playing;
        player.set_playing(self.playing);
    }

    fn stop(&mut self) {
        if let Some(player) = &self.player {
            player.seek_seconds(0.0);
            player.set_playing(false);
        }
        self.playing = false;
    }

    fn seek(&mut self, seconds: f32) {
        if let Some(player) = &self.player {
            player.seek_seconds(seconds.max(0.0));
        }
    }

    fn play_from_lyric(&mut self, index: usize) {
        let Some(line) = self.lyrics.get(index) else {
            return;
        };
        let Some(player) = &self.player else {
            self.error = Some(self.language.text().no_audio.to_owned());
            return;
        };

        player.seek_seconds(line.time);
        player.set_playing(true);
        self.playing = true;
    }

    fn calibrate_lyrics(&mut self) {
        let offset = self.calibration_seconds.parse::<i32>().unwrap_or(0) as f32;
        for line in &mut self.lyrics {
            line.time = (line.time + offset).max(0.0);
        }
        self.lyrics.sort_by(|a, b| a.time.total_cmp(&b.time));
    }

    fn sanitize_calibration_input(&mut self) {
        let mut value = String::new();
        for (index, ch) in self.calibration_seconds.chars().enumerate() {
            if ch.is_ascii_digit() || (index == 0 && (ch == '-' || ch == '+')) {
                value.push(ch);
            }
        }

        if value == "-" || value == "+" || value.is_empty() {
            self.calibration_seconds = value;
            return;
        }

        if value.parse::<i32>().is_ok() {
            self.calibration_seconds = value;
        } else {
            self.calibration_seconds = "0".to_owned();
        }
    }

    fn export_lrc(&mut self) {
        if self.lyrics.is_empty() {
            return;
        }

        let text = self.language.text();
        let default_name = self
            .lrc_path
            .as_deref()
            .and_then(Path::file_stem)
            .and_then(|name| name.to_str())
            .map(|name| format!("{name}_calibrated.lrc"))
            .unwrap_or_else(|| "lyrics_calibrated.lrc".to_owned());

        let Some(path) = rfd::FileDialog::new()
            .add_filter(text.lyric_filter, &["lrc"])
            .set_file_name(&default_name)
            .save_file()
        else {
            return;
        };

        let content = self
            .lyrics
            .iter()
            .map(|line| format!("[{}]{}\n", format_lrc_time(line.time), line.text))
            .collect::<String>();

        match std::fs::write(&path, content) {
            Ok(_) => self.error = Some(format!("{}: {}", text.export_success, path.display())),
            Err(err) => self.error = Some(format!("{}: {err:#}", text.export_error)),
        }
    }

    fn total_duration(&self) -> f32 {
        self.tracks
            .iter()
            .flatten()
            .map(|track| track.audio.duration_seconds())
            .fold(0.0, f32::max)
    }
}

impl eframe::App for KaraokeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(16));
        self.handle_file_drop(ctx);
        let text = self.language.text();

        TopBottomPanel::top("toolbar")
            .frame(Frame::none().fill(Color32::from_rgb(246, 247, 249)))
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(8.0);
                    ui.heading(RichText::new(text.app_title).color(Color32::from_rgb(38, 47, 56)));
                    ui.separator();

                    ui.selectable_value(&mut self.language, Language::Chinese, "\u{4e2d}\u{6587}");
                    ui.selectable_value(&mut self.language, Language::English, "English");
                    ui.separator();

                    let play_text = if self.playing { text.pause } else { text.play };
                    if ui
                        .add_enabled(
                            self.player.is_some(),
                            Button::new(play_text).min_size([72.0, 34.0].into()),
                        )
                        .clicked()
                    {
                        self.toggle_play();
                    }
                    if ui.button(text.stop).clicked() {
                        self.stop();
                    }

                    if ui.button(text.choose_lyrics).clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter(
                                text.lyric_filter,
                                &["lrc", "txt", "text", "lyric", "lyrics"],
                            )
                            .pick_file()
                        {
                            self.load_lrc(path);
                        }
                    }

                    if ui
                        .add_enabled(!self.lyrics.is_empty(), Button::new(text.save_lyrics_as))
                        .clicked()
                    {
                        self.export_lrc();
                    }

                    for index in 0..2 {
                        if ui
                            .button(format!("{} {}", text.choose_audio, index + 1))
                            .clicked()
                        {
                            if let Some(path) = rfd::FileDialog::new().pick_file() {
                                self.load_track(index, path);
                            }
                        }
                    }
                });
                ui.add_space(8.0);
            });

        CentralPanel::default()
            .frame(Frame::none().fill(Color32::from_rgb(236, 239, 242)))
            .show(ctx, |ui| {
                ui.add_space(12.0);
                self.draw_import_cards(ui);
                ui.add_space(12.0);
                self.draw_status(ui);
                ui.add_space(12.0);
                self.draw_waveforms(ui);
                ui.add_space(14.0);
                self.draw_lyrics(ui);
            });

        self.draw_drop_overlay(ctx);
    }
}

impl KaraokeApp {
    fn handle_file_drop(&mut self, ctx: &egui::Context) {
        let paths = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect::<Vec<_>>()
        });

        if !paths.is_empty() {
            self.load_dropped_files(paths);
        }
    }

    fn draw_import_cards(&mut self, ui: &mut egui::Ui) {
        let text = self.language.text();
        ui.horizontal(|ui| {
            let lrc_label = display_name(self.lrc_path.as_deref(), text.click_or_drop);
            self.import_card(
                ui,
                text.lyric_card,
                &lrc_label,
                Color32::from_rgb(67, 150, 122),
                |this| {
                    let text = this.language.text();
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter(
                            text.lyric_filter,
                            &["lrc", "txt", "text", "lyric", "lyrics"],
                        )
                        .pick_file()
                    {
                        this.load_lrc(path);
                    }
                },
            );

            for index in 0..2 {
                let title = format!("{} {}", text.audio_track, index + 1);
                let color = if index == 0 {
                    Color32::from_rgb(86, 153, 196)
                } else {
                    Color32::from_rgb(150, 112, 190)
                };
                let path = self.tracks[index]
                    .as_ref()
                    .map(|track| track.path.as_path());
                let file_label = display_name(path, text.click_or_drop);
                self.import_card(ui, &title, &file_label, color, |this| {
                    if let Some(path) = rfd::FileDialog::new().pick_file() {
                        this.load_track(index, path);
                    }
                });
            }
        });
    }

    fn import_card(
        &mut self,
        ui: &mut egui::Ui,
        title: &str,
        file_label: &str,
        accent: Color32,
        on_click: impl FnOnce(&mut Self),
    ) {
        let width = (ui.available_width() / 3.0 - 8.0).max(180.0);
        let frame = Frame::none()
            .fill(Color32::WHITE)
            .stroke(Stroke::new(1.0, Color32::from_rgb(214, 220, 226)))
            .rounding(Rounding::same(8.0))
            .inner_margin(Margin::same(14.0));

        let response = frame
            .show(ui, |ui| {
                ui.set_min_size(Vec2::new(width, 92.0));
                ui.horizontal(|ui| {
                    let dot_rect = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover()).0;
                    ui.painter().circle_filled(dot_rect.center(), 6.0, accent);
                    ui.label(RichText::new(title).strong().size(17.0));
                });
                ui.add_space(10.0);
                ui.label(RichText::new(file_label).color(Color32::from_rgb(70, 78, 88)));
            })
            .response
            .interact(Sense::click());

        if response.clicked() {
            on_click(self);
        }
    }

    fn draw_status(&mut self, ui: &mut egui::Ui) {
        let text = self.language.text();
        Frame::none()
            .fill(Color32::WHITE)
            .stroke(Stroke::new(1.0, Color32::from_rgb(218, 224, 230)))
            .rounding(Rounding::same(8.0))
            .inner_margin(Margin::same(12.0))
            .show(ui, |ui| {
                let current = self.current_time();
                let duration = self.total_duration();
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format_time(current)).strong());
                    let mut seek = current;
                    let response = ui.add_sized(
                        [ui.available_width().max(160.0), 22.0],
                        egui::Slider::new(&mut seek, 0.0..=duration.max(1.0)).show_value(false),
                    );
                    if response.changed() {
                        self.seek(seek);
                    }
                    ui.label(format_time(duration));
                });

                if let Some(error) = &self.error {
                    ui.add_space(6.0);
                    ui.colored_label(Color32::from_rgb(198, 54, 54), error);
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let response = ui.add_sized(
                        [76.0, 24.0],
                        egui::TextEdit::singleline(&mut self.calibration_seconds),
                    );
                    if response.changed() {
                        self.sanitize_calibration_input();
                    }
                    ui.label(text.seconds_unit);
                    if ui
                        .add_enabled(!self.lyrics.is_empty(), Button::new(text.calibrate))
                        .clicked()
                    {
                        self.calibrate_lyrics();
                    }
                });
            });
    }

    fn draw_waveforms(&mut self, ui: &mut egui::Ui) {
        let height = 240.0;
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 8.0, Color32::from_rgb(19, 24, 31));

        let duration = self.total_duration().max(0.001);
        let current = self.current_time().min(duration);

        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let ratio = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                self.seek(ratio * duration);
            }
        }

        for index in 0..2 {
            let top = rect.top() + index as f32 * rect.height() / 2.0;
            let band = Rect::from_min_size(
                Pos2::new(rect.left(), top),
                Vec2::new(rect.width(), rect.height() / 2.0),
            )
            .shrink2(Vec2::new(16.0, 12.0));
            self.draw_single_waveform(ui, band, index);
        }

        let x = rect.left() + rect.width() * (current / duration);
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            Stroke::new(2.0, Color32::from_rgb(255, 209, 102)),
        );
        painter.circle_filled(
            Pos2::new(x, rect.top() + 14.0),
            5.0,
            Color32::from_rgb(255, 209, 102),
        );
    }

    fn draw_single_waveform(&self, ui: &egui::Ui, rect: Rect, index: usize) {
        let text = self.language.text();
        let painter = ui.painter();
        let mid_y = rect.center().y;
        painter.line_segment(
            [
                Pos2::new(rect.left(), mid_y),
                Pos2::new(rect.right(), mid_y),
            ],
            Stroke::new(1.0, Color32::from_gray(72)),
        );

        painter.text(
            Pos2::new(rect.left(), rect.top()),
            Align2::LEFT_TOP,
            format!("{} {}", text.audio_track, index + 1),
            FontId::proportional(14.0),
            Color32::from_gray(184),
        );

        let Some(track) = &self.tracks[index] else {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                text.waiting_audio,
                FontId::proportional(16.0),
                Color32::from_gray(122),
            );
            return;
        };

        let color = if index == 0 {
            Color32::from_rgb(84, 190, 160)
        } else {
            Color32::from_rgb(132, 158, 235)
        };

        let waveform = &track.waveform;
        if waveform.is_empty() {
            return;
        }

        let step = rect.width() / waveform.len().max(1) as f32;
        for (i, amp) in waveform.iter().enumerate() {
            let x = rect.left() + i as f32 * step;
            let y = amp.clamp(0.0, 1.0) * rect.height() * 0.45;
            painter.line_segment(
                [Pos2::new(x, mid_y - y), Pos2::new(x, mid_y + y)],
                Stroke::new(step.max(1.0), color),
            );
        }
    }

    fn draw_lyrics(&mut self, ui: &mut egui::Ui) {
        let text = self.language.text();
        let rect = ui.available_rect_before_wrap();
        let height = rect.height().max(260.0);
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 8.0, Color32::WHITE);

        if self.lyrics.is_empty() {
            self.lyric_drag_start = None;
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                text.waiting_lyrics,
                FontId::proportional(24.0),
                Color32::from_gray(120),
            );
            return;
        }

        let current = self.current_time();
        let active = active_lyric_index(&self.lyrics, current);
        let line_height = 42.0;
        let center_y = rect.center().y;
        let scrollbar_rect = Rect::from_min_max(
            Pos2::new(rect.right() - 20.0, rect.top() + 18.0),
            Pos2::new(rect.right() - 8.0, rect.bottom() - 18.0),
        );
        let content_rect = Rect::from_min_max(
            rect.min,
            Pos2::new(scrollbar_rect.left() - 8.0, rect.bottom()),
        );
        let scrollbar_response = ui.interact(
            scrollbar_rect,
            ui.id().with("lyric_scrollbar"),
            Sense::drag(),
        );

        if response.drag_started() {
            self.lyric_drag_start = Some(active);
        }

        if response.dragged() && !scrollbar_response.dragged() {
            let start = self.lyric_drag_start.unwrap_or(active);
            let line_delta = (-response.drag_delta().y / line_height).round() as isize;
            let target = start
                .saturating_add_signed(line_delta)
                .min(self.lyrics.len().saturating_sub(1));

            self.lyric_drag_preview = Some(target);
        }

        if scrollbar_response.dragged() {
            if let Some(pos) = scrollbar_response.interact_pointer_pos() {
                let ratio =
                    ((pos.y - scrollbar_rect.top()) / scrollbar_rect.height()).clamp(0.0, 1.0);
                let target = (ratio * self.lyrics.len().saturating_sub(1) as f32).round() as usize;
                self.lyric_drag_preview = Some(target.min(self.lyrics.len().saturating_sub(1)));
            }
        }

        let mut display_active = self.lyric_drag_preview.unwrap_or(active);

        if response.double_clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                if !scrollbar_rect.contains(pos) {
                    let offset = ((pos.y - center_y) / line_height).round() as isize;
                    if (-3..=3).contains(&offset) {
                        if let Some(index) = display_active.checked_add_signed(offset) {
                            if index < self.lyrics.len() {
                                self.lyric_drag_preview = None;
                                self.lyric_drag_start = None;
                                self.play_from_lyric(index);
                                display_active = index;
                            }
                        }
                    }
                }
            }
        }

        let pointer_down = ui.input(|input| input.pointer.any_down());
        if !pointer_down {
            if let Some(index) = self.lyric_drag_preview.take() {
                self.seek(self.lyrics[index].time);
                display_active = index;
            }
            self.lyric_drag_start = None;
        }

        self.draw_lyric_scrollbar(ui, scrollbar_rect, display_active);

        for offset in -3..=3 {
            let Some(index) = display_active.checked_add_signed(offset) else {
                continue;
            };
            let Some(line) = self.lyrics.get(index) else {
                continue;
            };
            let distance = offset.unsigned_abs() as f32;
            let y = center_y + offset as f32 * line_height;
            let is_active = offset == 0;
            let alpha = (255.0 - distance * 48.0).max(88.0) as u8;
            let color = if is_active {
                Color32::from_rgb(31, 111, 92)
            } else {
                Color32::from_rgba_unmultiplied(83, 92, 102, alpha)
            };
            let font = if is_active {
                FontId::proportional(30.0)
            } else {
                FontId::proportional(20.0)
            };
            painter.text(
                Pos2::new(content_rect.center().x, y),
                Align2::CENTER_CENTER,
                &line.text,
                font,
                color,
            );
        }
    }

    fn draw_lyric_scrollbar(&self, ui: &egui::Ui, rect: Rect, active: usize) {
        let painter = ui.painter();
        painter.rect_filled(rect, 6.0, Color32::from_rgb(226, 231, 236));

        let ratio = if self.lyrics.len() <= 1 {
            0.0
        } else {
            active as f32 / self.lyrics.len().saturating_sub(1) as f32
        };
        let knob_height = 42.0_f32.min(rect.height()).max(24.0);
        let knob_top = rect.top() + (rect.height() - knob_height) * ratio;
        let knob = Rect::from_min_size(
            Pos2::new(rect.left(), knob_top),
            Vec2::new(rect.width(), knob_height),
        );

        painter.rect_filled(knob, 6.0, Color32::from_rgb(31, 111, 92));
    }

    fn draw_drop_overlay(&self, ctx: &egui::Context) {
        let has_hovered_files = ctx.input(|input| !input.raw.hovered_files.is_empty());
        if !has_hovered_files {
            return;
        }

        let text = self.language.text();
        let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop_overlay"));
        let painter = ctx.layer_painter(layer);
        let rect = ctx.input(|input| input.screen_rect());
        painter.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(22, 28, 36, 190));
        painter.rect_stroke(
            rect.shrink(28.0),
            12.0,
            Stroke::new(2.0, Color32::from_rgb(255, 209, 102)),
        );
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            text.drop_to_import,
            FontId::proportional(34.0),
            Color32::WHITE,
        );
    }
}

fn configure_chinese_fonts(ctx: &egui::Context) {
    let Some(font_bytes) = load_system_chinese_font() else {
        return;
    };

    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "system_chinese".to_owned(),
        FontData::from_owned(font_bytes),
    );

    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "system_chinese".to_owned());
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "system_chinese".to_owned());

    ctx.set_fonts(fonts);
}

fn load_system_chinese_font() -> Option<Vec<u8>> {
    let candidates = [
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\msyh.ttf",
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\simsun.ttc",
        r"C:\Windows\Fonts\Deng.ttf",
        r"C:\Windows\Fonts\NotoSansCJK-Regular.ttc",
    ];

    candidates.iter().find_map(|path| std::fs::read(path).ok())
}

#[derive(Clone)]
struct AudioData {
    samples: Vec<f32>,
    channels: usize,
    sample_rate: u32,
}

impl AudioData {
    fn silence_like(track: &Track) -> Self {
        Self {
            samples: Vec::new(),
            channels: track.audio.channels,
            sample_rate: track.audio.sample_rate,
        }
    }

    fn duration_seconds(&self) -> f32 {
        if self.channels == 0 || self.sample_rate == 0 {
            return 0.0;
        }
        self.samples.len() as f32 / self.channels as f32 / self.sample_rate as f32
    }
}

struct Track {
    path: PathBuf,
    audio: Arc<AudioData>,
    waveform: Vec<f32>,
}

struct AudioPlayer {
    _stream: Stream,
    playing: Arc<AtomicBool>,
    frame_position: Arc<AtomicU64>,
    output_rate: u32,
}

impl AudioPlayer {
    fn new(tracks: [Arc<AudioData>; 2]) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow!("No default output device found"))?;
        let supported_config = device.default_output_config()?;
        let output_rate = supported_config.sample_rate().0;
        let stream_config: StreamConfig = supported_config.clone().into();
        let output_channels = stream_config.channels as usize;

        let playing = Arc::new(AtomicBool::new(false));
        let frame_position = Arc::new(AtomicU64::new(0));
        let shared = Arc::new(Mutex::new(AudioState {
            tracks,
            playing: playing.clone(),
            frame_position: frame_position.clone(),
            output_rate,
            output_channels,
        }));

        let err_fn = |err| eprintln!("audio stream error: {err}");
        let stream = match supported_config.sample_format() {
            SampleFormat::F32 => build_stream::<f32>(&device, &stream_config, shared, err_fn)?,
            SampleFormat::I16 => build_stream::<i16>(&device, &stream_config, shared, err_fn)?,
            SampleFormat::U16 => build_stream::<u16>(&device, &stream_config, shared, err_fn)?,
            format => return Err(anyhow!("Unsupported output sample format: {format:?}")),
        };
        stream.play()?;

        Ok(Self {
            _stream: stream,
            playing,
            frame_position,
            output_rate,
        })
    }

    fn set_playing(&self, playing: bool) {
        self.playing.store(playing, Ordering::SeqCst);
    }

    fn seek_seconds(&self, seconds: f32) {
        let frame = (seconds * self.output_rate as f32).round().max(0.0) as u64;
        self.frame_position.store(frame, Ordering::SeqCst);
    }

    fn current_seconds(&self) -> f32 {
        self.frame_position.load(Ordering::SeqCst) as f32 / self.output_rate as f32
    }
}

struct AudioState {
    tracks: [Arc<AudioData>; 2],
    playing: Arc<AtomicBool>,
    frame_position: Arc<AtomicU64>,
    output_rate: u32,
    output_channels: usize,
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    state: Arc<Mutex<AudioState>>,
    err_fn: impl Fn(cpal::StreamError) + Send + 'static,
) -> Result<Stream>
where
    T: Sample + SizedSample + FromSample<f32>,
{
    let stream = device.build_output_stream(
        config,
        move |output: &mut [T], _| {
            if let Ok(mut state) = state.lock() {
                write_audio::<T>(output, &mut state);
            } else {
                for sample in output {
                    *sample = T::from_sample(0.0);
                }
            }
        },
        err_fn,
        None,
    )?;
    Ok(stream)
}

fn write_audio<T>(output: &mut [T], state: &mut AudioState)
where
    T: Sample + FromSample<f32>,
{
    if !state.playing.load(Ordering::SeqCst) {
        for sample in output {
            *sample = T::from_sample(0.0);
        }
        return;
    }

    let mut frame = state.frame_position.load(Ordering::SeqCst);
    let total_frames = output.len() / state.output_channels.max(1);

    for frame_samples in output.chunks_mut(state.output_channels.max(1)) {
        for (channel, sample) in frame_samples.iter_mut().enumerate() {
            let mut mixed = 0.0;
            for track in &state.tracks {
                mixed += sample_at(track, frame, channel, state.output_rate);
            }
            mixed = (mixed * 0.55).clamp(-1.0, 1.0);
            *sample = T::from_sample(mixed);
        }
        frame += 1;
    }

    state
        .frame_position
        .fetch_add(total_frames as u64, Ordering::SeqCst);
}

fn sample_at(track: &AudioData, output_frame: u64, output_channel: usize, output_rate: u32) -> f32 {
    if track.samples.is_empty() || track.channels == 0 || track.sample_rate == 0 {
        return 0.0;
    }
    let source_frame = output_frame as f64 * track.sample_rate as f64 / output_rate as f64;
    let source_frame = source_frame as usize;
    let source_channel = output_channel.min(track.channels - 1);
    let index = source_frame * track.channels + source_channel;
    track.samples.get(index).copied().unwrap_or(0.0)
}

#[derive(Clone)]
struct LyricLine {
    time: f32,
    text: String,
}

fn parse_lrc_file(path: &Path) -> Result<Vec<LyricLine>> {
    let content = read_text_file_lossy(path)?;
    let mut lines = Vec::new();
    let mut plain_lines = Vec::new();

    for raw_line in content.lines() {
        let mut rest = raw_line.trim();
        let mut times = Vec::new();

        while let Some(stripped) = rest.strip_prefix('[') {
            let Some(end) = stripped.find(']') else {
                break;
            };
            let tag = &stripped[..end];
            if let Some(seconds) = parse_lrc_timestamp(tag) {
                times.push(seconds);
            }
            rest = &stripped[end + 1..];
        }

        let text = rest.trim();
        if text.is_empty() {
            continue;
        }

        if times.is_empty() {
            plain_lines.push(text.to_owned());
        } else {
            for time in times {
                lines.push(LyricLine {
                    time,
                    text: text.to_owned(),
                });
            }
        }
    }

    if lines.is_empty() {
        for (index, text) in plain_lines.into_iter().enumerate() {
            lines.push(LyricLine {
                time: index as f32 * 3.0,
                text,
            });
        }
    } else {
        lines.sort_by(|a, b| a.time.total_cmp(&b.time));
    }

    Ok(lines)
}

fn read_text_file_lossy(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("Read {}", path.display()))?;

    if let Some(text) = decode_utf_bom(&bytes) {
        return Ok(text);
    }

    if let Ok(text) = std::str::from_utf8(&bytes) {
        return Ok(text.trim_start_matches('\u{feff}').to_owned());
    }

    for label in [
        b"gb18030".as_slice(),
        b"gbk".as_slice(),
        b"big5".as_slice(),
        b"windows-1252".as_slice(),
        b"shift_jis".as_slice(),
        b"euc-kr".as_slice(),
    ] {
        if let Some(encoding) = Encoding::for_label(label) {
            let (text, _, had_errors) = encoding.decode(&bytes);
            if !had_errors {
                return Ok(text.into_owned());
            }
        }
    }

    let (text, _, _) = GB18030.decode(&bytes);
    Ok(text.into_owned())
}

fn decode_utf_bom(bytes: &[u8]) -> Option<String> {
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        return Some(String::from_utf8_lossy(&bytes[3..]).into_owned());
    }

    if bytes.starts_with(&[0xff, 0xfe]) {
        return Some(decode_utf16_lossy(&bytes[2..], true));
    }

    if bytes.starts_with(&[0xfe, 0xff]) {
        return Some(decode_utf16_lossy(&bytes[2..], false));
    }

    None
}

fn decode_utf16_lossy(bytes: &[u8], little_endian: bool) -> String {
    let units = bytes
        .chunks_exact(2)
        .map(|chunk| {
            if little_endian {
                u16::from_le_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_be_bytes([chunk[0], chunk[1]])
            }
        })
        .collect::<Vec<_>>();

    String::from_utf16_lossy(&units)
}

fn parse_lrc_timestamp(tag: &str) -> Option<f32> {
    let (minutes, seconds) = tag.split_once(':')?;
    let minutes: f32 = minutes.parse().ok()?;
    let seconds: f32 = seconds.parse().ok()?;
    Some(minutes * 60.0 + seconds)
}

fn active_lyric_index(lyrics: &[LyricLine], seconds: f32) -> usize {
    match lyrics.binary_search_by(|line| line.time.total_cmp(&seconds)) {
        Ok(index) => index,
        Err(0) => 0,
        Err(index) => index - 1,
    }
}

fn decode_track(path: &Path) -> Result<Track> {
    let file = File::open(path).with_context(|| format!("Open {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
        hint.with_extension(extension);
    }

    let probed = symphonia::default::get_probe().format(
        &hint,
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|track| track.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
        .ok_or_else(|| anyhow!("No decodable audio track found"))?
        .clone();

    let sample_rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| anyhow!("Unable to detect sample rate"))?;
    let channels = track
        .codec_params
        .channels
        .ok_or_else(|| anyhow!("Unable to detect channel count"))?
        .count()
        .max(1);

    let mut decoder =
        symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;

    let mut samples = Vec::new();

    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track.id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(err) => return Err(err.into()),
        };

        append_audio_buffer(&decoded, channels, &mut samples);
    }

    let audio = Arc::new(AudioData {
        samples,
        channels,
        sample_rate,
    });
    let waveform = build_waveform(&audio, WAVEFORM_BUCKETS);

    Ok(Track {
        path: path.to_path_buf(),
        audio,
        waveform,
    })
}

fn append_audio_buffer(decoded: &AudioBufferRef<'_>, channels: usize, samples: &mut Vec<f32>) {
    match decoded {
        AudioBufferRef::F32(buffer) => append_typed_buffer(buffer, channels, samples),
        AudioBufferRef::U8(buffer) => append_typed_buffer(buffer, channels, samples),
        AudioBufferRef::U16(buffer) => append_typed_buffer(buffer, channels, samples),
        AudioBufferRef::U24(buffer) => append_typed_buffer(buffer, channels, samples),
        AudioBufferRef::U32(buffer) => append_typed_buffer(buffer, channels, samples),
        AudioBufferRef::S8(buffer) => append_typed_buffer(buffer, channels, samples),
        AudioBufferRef::S16(buffer) => append_typed_buffer(buffer, channels, samples),
        AudioBufferRef::S24(buffer) => append_typed_buffer(buffer, channels, samples),
        AudioBufferRef::S32(buffer) => append_typed_buffer(buffer, channels, samples),
        AudioBufferRef::F64(buffer) => append_typed_buffer(buffer, channels, samples),
    }
}

fn append_typed_buffer<S>(
    buffer: &symphonia::core::audio::AudioBuffer<S>,
    channels: usize,
    samples: &mut Vec<f32>,
) where
    S: SymphoniaSample,
    f32: SymphoniaFromSample<S>,
{
    let frames = buffer.frames();
    let spec_channels = buffer.spec().channels.count().max(1);
    for frame in 0..frames {
        for channel in 0..channels {
            let source_channel = channel.min(spec_channels - 1);
            samples.push(<f32 as SymphoniaFromSample<S>>::from_sample(
                buffer.chan(source_channel)[frame],
            ));
        }
    }
}

fn build_waveform(audio: &AudioData, buckets: usize) -> Vec<f32> {
    if audio.samples.is_empty() || audio.channels == 0 {
        return Vec::new();
    }

    let frames = audio.samples.len() / audio.channels;
    let bucket_count = buckets.min(frames.max(1));
    let frames_per_bucket = (frames as f32 / bucket_count as f32).ceil() as usize;
    let mut waveform = Vec::with_capacity(bucket_count);

    for bucket in 0..bucket_count {
        let start = bucket * frames_per_bucket;
        let end = ((bucket + 1) * frames_per_bucket).min(frames);
        let mut peak = 0.0_f32;

        for frame in start..end {
            let mut sum = 0.0;
            for channel in 0..audio.channels {
                sum += audio.samples[frame * audio.channels + channel].abs();
            }
            peak = peak.max(sum / audio.channels as f32);
        }

        waveform.push(peak.sqrt().clamp(0.0, 1.0));
    }

    waveform
}

fn is_lyric_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "lrc" | "txt" | "text" | "lyric" | "lyrics"
            )
        })
}

fn format_time(seconds: f32) -> String {
    let total = seconds.max(0.0).round() as u32;
    format!("{:02}:{:02}", total / 60, total % 60)
}

fn format_lrc_time(seconds: f32) -> String {
    let centiseconds = (seconds.max(0.0) * 100.0).round() as u32;
    let minutes = centiseconds / 6000;
    let seconds = (centiseconds % 6000) / 100;
    let hundredths = centiseconds % 100;
    format!("{minutes:02}:{seconds:02}.{hundredths:02}")
}

fn display_name(path: Option<&Path>, fallback: &str) -> String {
    path.and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .unwrap_or_else(|| fallback.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_temp_file(name: &str, bytes: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "lyrics_follow_player_{}_{}",
            std::process::id(),
            name
        ));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn reads_utf8_lrc() {
        let path = write_temp_file(
            "utf8.lrc",
            "[00:01.00]\u{4f60}\u{597d}\n[00:02.00]\u{4e16}\u{754c}".as_bytes(),
        );
        let lyrics = parse_lrc_file(&path).unwrap();
        std::fs::remove_file(path).ok();

        assert_eq!(lyrics.len(), 2);
        assert_eq!(lyrics[0].text, "\u{4f60}\u{597d}");
        assert!((lyrics[0].time - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn reads_gbk_text() {
        let path = write_temp_file(
            "gbk.txt",
            &[0xc4, 0xe3, 0xba, 0xc3, b'\n', 0xca, 0xc0, 0xbd, 0xe7],
        );
        let lyrics = parse_lrc_file(&path).unwrap();
        std::fs::remove_file(path).ok();

        assert_eq!(lyrics.len(), 2);
        assert_eq!(lyrics[0].text, "\u{4f60}\u{597d}");
        assert_eq!(lyrics[1].text, "\u{4e16}\u{754c}");
    }

    #[test]
    fn reads_utf16le_text() {
        let mut bytes = vec![0xff, 0xfe];
        for unit in "\u{4f60}\u{597d}\n\u{4e16}\u{754c}".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }

        let path = write_temp_file("utf16le.txt", &bytes);
        let lyrics = parse_lrc_file(&path).unwrap();
        std::fs::remove_file(path).ok();

        assert_eq!(lyrics.len(), 2);
        assert_eq!(lyrics[0].text, "\u{4f60}\u{597d}");
        assert_eq!(lyrics[1].text, "\u{4e16}\u{754c}");
    }
}
