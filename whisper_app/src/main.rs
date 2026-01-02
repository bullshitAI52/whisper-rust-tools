use eframe::egui;
use rfd::FileDialog;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::path::{Path, PathBuf};
use std::fs;
use common::time_utils::seconds_to_time_str;

mod audio;
mod whisper_engine;
use common::ai::DeepSeekClient;
use whisper_engine::WhisperEngine;

struct WhisperApp {
    // Tabs
    selected_tab: Tab,
    
    // Transcription State
    tx_files: Vec<String>,
    tx_model: String,
    tx_output_dir: String,
    is_transcribing: bool,
    
    // Engine State
    engine: Arc<Mutex<Option<WhisperEngine>>>,
    rx: Receiver<AppMessage>,
    tx: Sender<AppMessage>,
    
    // Logs
    logs: Vec<String>,
    
    // AI / DeepSeek
    deepseek_key: String,
    
    // Translation Tab State
    trans_input_file: String,
    trans_target_lang: String,
    
    // Storyboard Tab State
    story_input_file: String,
    story_prompt: String,
}

enum AppMessage {
    Log(String),
    ModelLoaded,
    TranscriptionDone(String), // Result message
}

#[derive(PartialEq, Eq)]
enum Tab {
    Transcription,
    Translation,
    Storyboard,
    Logs,
    Help,
}

impl Default for WhisperApp {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            selected_tab: Tab::Transcription,
            tx_files: vec![],
            tx_model: "small".to_string(),
            tx_output_dir: std::env::current_dir().unwrap().display().to_string(),
            is_transcribing: false,
            engine: Arc::new(Mutex::new(None)),
            rx,
            tx,
            logs: vec!["欢迎使用 Whisper Tool".to_string()],
            
            deepseek_key: std::env::var("DEEPSEEK_API_KEY").unwrap_or_default(),
            trans_input_file: String::new(),
            trans_target_lang: "English".to_owned(),
            story_input_file: String::new(),
            story_prompt: "Create a cinematic storyboard".to_owned(),
        }
    }
}

impl WhisperApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_custom_fonts(&cc.egui_ctx);
        Self::default()
    }
    
    fn log(&mut self, msg: &str) {
        self.logs.push(msg.to_string());
    }
    
    fn handle_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                AppMessage::Log(s) => self.log(&s),
                AppMessage::ModelLoaded => {
                    self.log("模型加载成功!");
                }
                AppMessage::TranscriptionDone(res) => {
                    self.log(&res);
                    self.is_transcribing = false;
                }
            }
        }
    }
}

impl eframe::App for WhisperApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_messages();
        
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Whisper Tool");
                ui.separator();
                ui.selectable_value(&mut self.selected_tab, Tab::Transcription, "🎤 转写");
                ui.selectable_value(&mut self.selected_tab, Tab::Translation, "🌐 翻译");
                ui.selectable_value(&mut self.selected_tab, Tab::Storyboard, "🎬 分镜");
                ui.selectable_value(&mut self.selected_tab, Tab::Logs, "📋 日志");
                ui.selectable_value(&mut self.selected_tab, Tab::Help, "❓ 帮助");
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.selected_tab {
                Tab::Transcription => self.show_transcription(ui),
                Tab::Translation => self.show_translation(ui),
                Tab::Storyboard => self.show_storyboard(ui),
                Tab::Logs => self.show_logs(ui),
                Tab::Help => self.show_help(ui),
            }
        });
        
        if self.is_transcribing {
            ctx.request_repaint();
        }
    }
}

impl WhisperApp {
    fn show_transcription(&mut self, ui: &mut egui::Ui) {
        ui.heading("语音转字幕");
        
        ui.horizontal(|ui| {
            egui::ComboBox::from_label("模型")
                .selected_text(&self.tx_model)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.tx_model, "tiny".to_string(), "Tiny (极速)");
                    ui.selectable_value(&mut self.tx_model, "base".to_string(), "Base (均衡)");
                    ui.selectable_value(&mut self.tx_model, "small".to_string(), "Small (推荐)");
                    ui.selectable_value(&mut self.tx_model, "medium".to_string(), "Medium");
                    ui.selectable_value(&mut self.tx_model, "large".to_string(), "Large");
                });
            
            if ui.button("加载模型").clicked() {
                let model_id = self.tx_model.clone();
                let tx = self.tx.clone();
                let engine = self.engine.clone();
                
                self.log(&format!("正在加载模型: {} (可能需要几分钟下载)...", model_id));
                
                tokio::spawn(async move {
                    match WhisperEngine::new(&model_id) {
                        Ok(e) => {
                            *engine.lock().await = Some(e);
                            let _ = tx.send(AppMessage::ModelLoaded);
                        },
                        Err(err) => {
                            let _ = tx.send(AppMessage::Log(format!("加载模型失败: {}", err)));
                        }
                    }
                });
            }
        });

        ui.horizontal(|ui| {
            ui.label("输出目录:");
            ui.text_edit_singleline(&mut self.tx_output_dir);
            if ui.button("浏览...").clicked() {
                if let Some(path) = FileDialog::new().pick_folder() {
                    self.tx_output_dir = path.display().to_string();
                }
            }
        });

        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("添加文件").clicked() {
                if let Some(files) = FileDialog::new().pick_files() {
                    for f in files {
                        self.tx_files.push(f.display().to_string());
                    }
                }
            }
            if ui.button("清空列表").clicked() {
                self.tx_files.clear();
            }
        });

        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            for f in &self.tx_files {
                ui.label(f);
            }
        });

        ui.separator();
        if ui.button(if self.is_transcribing { "⏳ 转写中..." } else { "▶️ 开始转写" }).clicked() {
            if !self.is_transcribing {
                if self.tx_files.is_empty() {
                    self.log("未选择文件!");
                    return;
                }
                
                self.is_transcribing = true;
                self.log("开始转写队列...");
                
                let files = self.tx_files.clone();
                let engine = self.engine.clone();
                let tx = self.tx.clone();
                let output_dir = self.tx_output_dir.clone();
                
                tokio::spawn(async move {
                    let mut guard = engine.lock().await;
                    if let Some(engine) = guard.as_mut() {
                        for file in files {
                            let _ = tx.send(AppMessage::Log(format!("正在处理: {}", file)));
                            match engine.transcribe(&file) {
                                Ok(segments) => {
                                    let mut srt_content = String::new();
                                    for (i, (start, end, text)) in segments.iter().enumerate() {
                                        srt_content.push_str(&format!(
                                            "{}\n{} --> {}\n{}\n\n",
                                            i + 1,
                                            seconds_to_time_str(*start),
                                            seconds_to_time_str(*end),
                                            text.trim()
                                        ));
                                    }
                                    
                                    let input_path = Path::new(&file);
                                    let file_stem = input_path.file_stem().unwrap().to_string_lossy();
                                    let output_path = Path::new(&output_dir).join(format!("{}.srt", file_stem));
                                    
                                    if let Err(e) = fs::write(&output_path, srt_content) {
                                         let _ = tx.send(AppMessage::Log(format!("保存 SRT 失败: {}", e)));
                                    } else {
                                         let _ = tx.send(AppMessage::Log(format!("SRT 已保存至: {}", output_path.display())));
                                    }
                                }
                                Err(e) => {
                                    let _ = tx.send(AppMessage::Log(format!("处理失败 {}: {}", file, e)));
                                }
                            }
                        }
                        let _ = tx.send(AppMessage::TranscriptionDone("所有文件处理完毕。".to_string()));
                    } else {
                        let _ = tx.send(AppMessage::TranscriptionDone("错误: 模型未加载! 请先点击加载模型。".to_string()));
                    }
                });
            }
        }
    }

    fn show_translation(&mut self, ui: &mut egui::Ui) {
        ui.heading("字幕翻译 (AI)");
        ui.separator();
        
        ui.horizontal(|ui| {
            ui.label("DeepSeek Key:");
            ui.add(egui::TextEdit::singleline(&mut self.deepseek_key).password(true));
        });
        
        ui.separator();
        
        ui.horizontal(|ui| {
            ui.label("输入字幕 (.srt):");
            ui.text_edit_singleline(&mut self.trans_input_file);
            if ui.button("浏览 file").clicked() {
                if let Some(path) = FileDialog::new().add_filter("SRT", &["srt"]).pick_file() {
                    self.trans_input_file = path.display().to_string();
                }
            }
        });
        
        ui.horizontal(|ui| {
            ui.label("目标语言:");
            ui.text_edit_singleline(&mut self.trans_target_lang); 
            // Could use combobox, but text is flexible
        });
        
        if ui.button("🚀 开始翻译").clicked() {
            let key = self.deepseek_key.clone();
            let file = self.trans_input_file.clone();
            let lang = self.trans_target_lang.clone();
            let tx = self.tx.clone();
            
            if file.is_empty() {
                self.log("请选择 SRT 文件");
                return;
            }
            
            self.log("开始翻译任务...");
            tokio::spawn(async move {
                if let Ok(content) = fs::read_to_string(&file) {
                    let client = DeepSeekClient::new(key);
                    // Simple logic: translate whole block. Chunking is better but complex for now.
                    match client.translate(&content, &lang).await {
                         Ok(translated) => {
                             let out_path = file.replace(".srt", &format!("_{}.srt", lang));
                             if let Ok(_) = fs::write(&out_path, translated) {
                                  let _ = tx.send(AppMessage::Log(format!("翻译保存至: {}", out_path)));
                             } else {
                                  let _ = tx.send(AppMessage::Log("保存失败".to_string()));
                             }
                         }
                         Err(e) => {
                             let _ = tx.send(AppMessage::Log(format!("翻译 API 错误: {}", e)));
                         }
                    }
                } else {
                    let _ = tx.send(AppMessage::Log("无法读取文件".to_string()));
                }
            });
        }
    }

    fn show_storyboard(&mut self, ui: &mut egui::Ui) {
        ui.heading("分镜生成 (AI)");
        ui.separator();
        
        ui.horizontal(|ui| {
            ui.label("DeepSeek Key:");
            ui.add(egui::TextEdit::singleline(&mut self.deepseek_key).password(true));
        });
        
        ui.horizontal(|ui| {
            ui.label("输入文本/字幕:");
            ui.text_edit_singleline(&mut self.story_input_file);
            if ui.button("浏览 file").clicked() {
               if let Some(path) = FileDialog::new().add_filter("Text", &["txt", "srt"]).pick_file() {
                    self.story_input_file = path.display().to_string();
                } 
            }
        });
        
        ui.horizontal(|ui| {
            ui.label("提示词风格:");
            ui.text_edit_singleline(&mut self.story_prompt);
        });
        
        if ui.button("🎨 生成分镜 Prompt").clicked() {
             let key = self.deepseek_key.clone();
             let file = self.story_input_file.clone();
             let tx = self.tx.clone();
             
             if file.is_empty() {
                 self.log("请选择输入文件");
                 return;
             }
             
             self.log("正在生成分镜描述...");
             tokio::spawn(async move {
                 if let Ok(content) = fs::read_to_string(&file) {
                     let client = DeepSeekClient::new(key);
                     match client.generate_storyboard(&content).await {
                         Ok(res) => {
                             let out_path = file.replace(".srt", "_storyboard.txt").replace(".txt", "_storyboard.txt");
                             if let Ok(_) = fs::write(&out_path, res) {
                                  let _ = tx.send(AppMessage::Log(format!("分镜已保存: {}", out_path)));
                             }
                         }
                         Err(e) => {
                             let _ = tx.send(AppMessage::Log(format!("API 错误: {}", e)));
                         }
                     }
                 }
             });
        }
    }

    fn show_logs(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            for log in &self.logs {
                ui.monospace(log);
            }
        });
    }

    fn show_help(&self, ui: &mut egui::Ui) {
        ui.heading("使用说明 / Help");
        ui.separator();
        
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.label(egui::RichText::new("1. 🎤 语音转字幕 (Transcription)").strong());
            ui.label("   - **步骤**: 选择模型 -> 加载模型 -> 添加音频/视频 -> 开始转写。");
            ui.label("   - **模型**: 推荐使用 Small。第一次加载会自动下载。");
            ui.label("   - **输出**: 默认输出到与输入文件同名的 .srt 文件。");
            ui.add_space(10.0);
            
            ui.label(egui::RichText::new("2. 🌐 字幕翻译 (Translation)").strong());
            ui.label("   - **前提**: 需要 DeepSeek API Key (可设置环境变量 DEEPSEEK_API_KEY)。");
            ui.label("   - **步骤**: 选择 .srt 文件 -> 输入目标语言 -> 点击开始翻译。");
            ui.add_space(10.0);
            
            ui.label(egui::RichText::new("3. 🎬 分镜生成 (Storyboard)").strong());
            ui.label("   - **功能**: 根据字幕或文本生成 AI 绘画 (Midjourney) 的提示词。");
            ui.add_space(10.0);
            
            ui.label(egui::RichText::new("⚠️ 注意事项").color(egui::Color32::RED));
            ui.label("   - 模型文件保存在 ~/.cache/huggingface/hub 下，较大。");
            ui.label("   - AI 功能依赖网络连接。");
        });
    }
}

fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    
    // Attempt to load system font for Chinese support (macOS primary)
    let font_candidates = [
        "/System/Library/Fonts/PingFang.ttc", // MacOS
        "/System/Library/Fonts/STHeiti Light.ttc", // MacOS Legacy
        "C:\\Windows\\Fonts\\msyh.ttc", // Windows YaHei
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", // Linux
    ];
    
    let mut font_data = None;
    for path in font_candidates {
        if let Ok(data) = std::fs::read(path) {
            font_data = Some(data);
            break;
        }
    }
    
    if let Some(data) = font_data {
        fonts.font_data.insert(
            "my_font".to_owned(),
            egui::FontData::from_owned(data).tweak(
                egui::FontTweak {
                    scale: 1.2, 
                    ..Default::default()
                }
            ),
        );
        
        fonts.families.entry(egui::FontFamily::Proportional).or_default()
            .insert(0, "my_font".to_owned());
        fonts.families.entry(egui::FontFamily::Monospace).or_default()
            .insert(0, "my_font".to_owned());
            
        ctx.set_fonts(fonts);
    }
}

#[tokio::main]
async fn main() -> eframe::Result<()> {
    env_logger::init();
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 700.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Whisper Tool (Rust)",
        native_options,
        Box::new(|cc| Ok(Box::new(WhisperApp::new(cc)))),
    )
}
