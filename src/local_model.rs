use crate::settings::ModelConfig;

#[cfg(feature = "local-model")]
mod with_local_model {
    use super::ModelConfig;
    use encoding_rs::UTF_8;
    use llama_cpp_2::context::params::LlamaContextParams;
    use llama_cpp_2::llama_backend::LlamaBackend;
    use llama_cpp_2::llama_batch::LlamaBatch;
    use llama_cpp_2::model::params::LlamaModelParams;
    use llama_cpp_2::model::{AddBos, LlamaChatMessage, LlamaModel};
    use llama_cpp_2::sampling::LlamaSampler;
    use once_cell::sync::Lazy;
    use serde::{Deserialize, Serialize};
    use std::fs::File;
    use std::io::{self, BufRead, BufReader, Read, Write};
    use std::num::NonZeroU32;
    use std::path::{Path, PathBuf};
    use std::process::{Child, ChildStdin, Command, Stdio};

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case")]
    enum WorkerLine {
        Ready {
            ok: bool,
            version: String,
            error: Option<String>,
        },
        Request {
            id: u64,
            system_prompt: String,
            prompt: String,
            max_tokens: u32,
        },
        Response {
            id: u64,
            ok: bool,
            text: Option<String>,
            error: Option<String>,
        },
    }

    struct WorkerClient {
        model_path: PathBuf,
        child: Child,
        stdin: ChildStdin,
        stdout: BufReader<std::process::ChildStdout>,
        next_id: u64,
    }

    static WORKER: Lazy<parking_lot::Mutex<Option<WorkerClient>>> =
        Lazy::new(|| parking_lot::Mutex::new(None));

    pub fn clear_cached_model() {
        let mut guard = WORKER.lock();
        if let Some(mut worker) = guard.take() {
            let _ = worker.child.kill();
            let _ = worker.child.wait();
        }
    }

    fn canonicalize_existing(path: &Path) -> Result<PathBuf, String> {
        if !path.exists() {
            return Err(format!("Model file not found: {}", path.display()));
        }
        let canonical = std::fs::canonicalize(path)
            .map_err(|e| format!("Could not resolve model path {}: {e}", path.display()))?;
        Ok(strip_windows_verbatim_prefix(&canonical))
    }

    #[cfg(not(windows))]
    fn strip_windows_verbatim_prefix(path: &Path) -> PathBuf {
        path.to_path_buf()
    }

    #[cfg(windows)]
    fn strip_windows_verbatim_prefix(path: &Path) -> PathBuf {
        let s = path.to_string_lossy();
        if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
            PathBuf::from(format!(r"\\{rest}"))
        } else if let Some(rest) = s.strip_prefix(r"\\?\") {
            PathBuf::from(rest)
        } else {
            path.to_path_buf()
        }
    }

    fn validate_gguf_magic(path: &Path) -> Result<(), String> {
        let mut f = File::open(path)
            .map_err(|e| format!("Could not open model file {}: {e}", path.display()))?;
        let mut magic = [0u8; 4];
        f.read_exact(&mut magic)
            .map_err(|e| format!("Could not read model file header {}: {e}", path.display()))?;
        if &magic != b"GGUF" {
            return Err(format!(
                "Model file does not look like GGUF (expected 'GGUF' magic): {}",
                path.display()
            ));
        }
        Ok(())
    }

    impl WorkerClient {
        fn spawn(model_path: &Path) -> Result<Self, String> {
            let exe = std::env::current_exe()
                .map_err(|e| format!("Could not locate current executable: {e}"))?;

            let mut child = Command::new(exe)
                .arg("--mode")
                .arg("model-worker")
                .arg("--model-path")
                .arg(model_path)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .map_err(|e| format!("Failed to start model worker process: {e}"))?;

            let stdin = child
                .stdin
                .take()
                .ok_or_else(|| "Failed to open worker stdin".to_string())?;
            let stdout = child
                .stdout
                .take()
                .ok_or_else(|| "Failed to open worker stdout".to_string())?;
            let mut stdout = BufReader::new(stdout);

            let mut first = String::new();
            let n = stdout
                .read_line(&mut first)
                .map_err(|e| format!("Failed reading from model worker: {e}"))?;
            if n == 0 {
                let status = child
                    .try_wait()
                    .ok()
                    .flatten()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                return Err(format!(
                    "Model worker exited before it became ready (status: {status}). \
This often means the GGUF is incompatible with this build's llama.cpp."
                ));
            }

            let ready: WorkerLine = serde_json::from_str(first.trim_end()).map_err(|e| {
                format!("Model worker sent an unexpected message while starting: {e}")
            })?;
            match ready {
                WorkerLine::Ready { ok: true, .. } => Ok(Self {
                    model_path: model_path.to_path_buf(),
                    child,
                    stdin,
                    stdout,
                    next_id: 1,
                }),
                WorkerLine::Ready {
                    ok: false, error, ..
                } => Err(error.unwrap_or_else(|| {
                    "Model worker could not start (unknown error).".to_string()
                })),
                other => Err(format!(
                    "Model worker sent an unexpected startup message: {:?}",
                    other
                )),
            }
        }

        fn is_running(&mut self) -> bool {
            self.child.try_wait().ok().flatten().is_none()
        }

        fn request(
            &mut self,
            system_prompt: &str,
            prompt: &str,
            max_tokens: u32,
        ) -> Result<String, String> {
            let id = self.next_id;
            self.next_id = self.next_id.saturating_add(1);

            let line = WorkerLine::Request {
                id,
                system_prompt: system_prompt.to_string(),
                prompt: prompt.to_string(),
                max_tokens: max_tokens.max(16),
            };
            let json =
                serde_json::to_string(&line).map_err(|e| format!("JSON encode error: {e}"))?;
            self.stdin
                .write_all(json.as_bytes())
                .and_then(|_| self.stdin.write_all(b"\n"))
                .and_then(|_| self.stdin.flush())
                .map_err(|e| format!("Failed sending request to model worker: {e}"))?;

            let mut resp = String::new();
            let n = self
                .stdout
                .read_line(&mut resp)
                .map_err(|e| format!("Failed reading model worker response: {e}"))?;
            if n == 0 {
                return Err(
                    "Model worker crashed while generating a response. The GGUF may be incompatible."
                        .to_string(),
                );
            }
            let decoded: WorkerLine = serde_json::from_str(resp.trim_end())
                .map_err(|e| format!("Model worker returned invalid JSON: {e}"))?;
            match decoded {
                WorkerLine::Response {
                    id: rid,
                    ok: true,
                    text: Some(text),
                    ..
                } if rid == id => Ok(text),
                WorkerLine::Response {
                    id: rid,
                    ok: false,
                    error,
                    ..
                } if rid == id => Err(error.unwrap_or_else(|| "Model worker error.".to_string())),
                other => Err(format!(
                    "Model worker returned an unexpected message: {:?}",
                    other
                )),
            }
        }
    }

    fn format_prompt(
        model: &LlamaModel,
        system_prompt: &str,
        user_input: &str,
    ) -> Result<String, String> {
        match model.chat_template(None) {
            Ok(tmpl) => {
                let chat = [
                    LlamaChatMessage::new("system".to_string(), system_prompt.to_string())
                        .map_err(|e| format!("Could not build system message: {e}"))?,
                    LlamaChatMessage::new("user".to_string(), user_input.to_string())
                        .map_err(|e| format!("Could not build user message: {e}"))?,
                ];
                model
                    .apply_chat_template(&tmpl, &chat, true)
                    .map_err(|e| format!("Could not apply model chat template: {e}"))
            }
            Err(_) => Ok(format!("{system_prompt}\n\nUser: {user_input}\nAssistant:")),
        }
    }

    fn direct_chat_completion(
        model: &LlamaModel,
        backend: &LlamaBackend,
        system_prompt: &str,
        user_input: &str,
        max_tokens: u32,
    ) -> Result<String, String> {
        let prompt = format_prompt(model, system_prompt, user_input)?;
        let mut tokens = model
            .str_to_token(&prompt, AddBos::Always)
            .map_err(|e| format!("Could not tokenize prompt: {e}"))?;

        let desired_ctx = model.n_ctx_train().max(2048).min(8192);
        let ctx_limit = desired_ctx as usize;
        if tokens.len() > ctx_limit.saturating_sub(8) {
            let keep = ctx_limit.saturating_sub(8);
            tokens = tokens[tokens.len().saturating_sub(keep)..].to_vec();
        }
        if tokens.is_empty() {
            return Err("Prompt tokenization returned no tokens".to_string());
        }

        let threads = std::thread::available_parallelism()
            .map(|n| n.get() as i32)
            .unwrap_or(1);
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(desired_ctx))
            .with_n_batch(2048)
            .with_n_ubatch(512)
            .with_n_threads(threads)
            .with_n_threads_batch(threads);

        let mut ctx = model
            .new_context(backend, ctx_params)
            .map_err(|e| format!("Failed to create model context: {e}"))?;

        let mut batch =
            LlamaBatch::get_one(&tokens).map_err(|e| format!("Failed to build batch: {e}"))?;
        ctx.decode(&mut batch)
            .map_err(|e| format!("Model decode failed: {e}"))?;
        let mut sample_idx = batch.n_tokens().saturating_sub(1);

        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::temp(0.3),
            LlamaSampler::top_p(0.9, 1),
            LlamaSampler::dist(0),
        ]);

        let mut decoder = UTF_8.new_decoder();
        let mut output = String::new();
        let mut gen_batch = LlamaBatch::new(1, 1);
        let mut pos = i32::try_from(tokens.len()).unwrap_or(i32::MAX);

        let max_predictions = max_tokens.max(16) as usize;
        let n_ctx = ctx.n_ctx() as i32;
        for _ in 0..max_predictions {
            if pos >= n_ctx.saturating_sub(1) {
                break;
            }
            let token = sampler.sample(&ctx, sample_idx);
            sampler.accept(token);

            if model.is_eog_token(token) {
                break;
            }

            if let Ok(piece) = model.token_to_piece(token, &mut decoder, false, None) {
                output.push_str(&piece);
            }

            gen_batch.clear();
            gen_batch
                .add(token, pos, &[0], true)
                .map_err(|e| format!("Failed to add token to batch: {e}"))?;
            ctx.decode(&mut gen_batch)
                .map_err(|e| format!("Model decode failed: {e}"))?;
            pos = pos.saturating_add(1);
            sample_idx = 0;
        }

        let cleaned = extract_final_message(&output).trim().to_string();
        if cleaned.is_empty() {
            Err("Model returned an empty response".to_string())
        } else {
            Ok(cleaned)
        }
    }

    fn extract_final_message(raw: &str) -> String {
        // Some GGUF chat templates (e.g. gpt-oss) emit channel markers like:
        // "<|channel|>analysis<|message|>..." then "<|channel|>final<|message|>...".
        // Prefer returning only the final message if present.
        const FINAL: &str = "<|channel|>final<|message|>";
        const ANALYSIS: &str = "<|channel|>analysis<|message|>";

        let mut s = if let Some(idx) = raw.rfind(FINAL) {
            &raw[idx + FINAL.len()..]
        } else if let Some(idx) = raw.find(ANALYSIS) {
            &raw[idx + ANALYSIS.len()..]
        } else {
            raw
        };

        // Trim common tag terminators if present.
        for cut in ["<|end|>", "<|eot|>", "<|start|>"].iter() {
            if let Some(idx) = s.find(cut) {
                s = &s[..idx];
            }
        }

        s.trim().to_string()
    }

    pub fn chat_completion(cfg: &ModelConfig, user_input: &str) -> Result<String, String> {
        chat_completion_with_system_prompt(
            cfg,
            "You are Chatty-EDU, an offline school AI helper. Answer plainly, safely, and briefly.",
            user_input,
        )
    }

    pub fn chat_completion_with_system_prompt(
        cfg: &ModelConfig,
        system_prompt: &str,
        user_input: &str,
    ) -> Result<String, String> {
        let wanted_path = canonicalize_existing(Path::new(&cfg.path))?;
        validate_gguf_magic(&wanted_path)?;

        let mut guard = WORKER.lock();
        let needs_restart = match guard.as_mut() {
            Some(worker) => worker.model_path != wanted_path || !worker.is_running(),
            None => true,
        };
        if needs_restart {
            if let Some(mut old) = guard.take() {
                let _ = old.child.kill();
                let _ = old.child.wait();
            }
            let worker = WorkerClient::spawn(&wanted_path)?;
            *guard = Some(worker);
        }

        let worker = guard
            .as_mut()
            .ok_or_else(|| "Model worker not available.".to_string())?;
        worker.request(system_prompt, user_input, cfg.max_tokens)
    }

    pub fn run_model_worker(model_path: &Path, default_max_tokens: u32) -> i32 {
        let version = env!("CARGO_PKG_VERSION").to_string();
        let resolved = match canonicalize_existing(model_path).and_then(|p| {
            validate_gguf_magic(&p)?;
            Ok(p)
        }) {
            Ok(p) => p,
            Err(err) => {
                let ready = WorkerLine::Ready {
                    ok: false,
                    version,
                    error: Some(err),
                };
                let _ = writeln!(io::stdout(), "{}", serde_json::to_string(&ready).unwrap());
                let _ = io::stdout().flush();
                return 2;
            }
        };

        let backend = match LlamaBackend::init() {
            Ok(b) => b,
            Err(e) => {
                let ready = WorkerLine::Ready {
                    ok: false,
                    version,
                    error: Some(format!("Failed to initialize llama backend: {e}")),
                };
                let _ = writeln!(io::stdout(), "{}", serde_json::to_string(&ready).unwrap());
                let _ = io::stdout().flush();
                return 3;
            }
        };

        let model_params = LlamaModelParams::default().with_n_gpu_layers(0);
        let model = match LlamaModel::load_from_file(&backend, &resolved, &model_params) {
            Ok(m) => m,
            Err(e) => {
                let ready = WorkerLine::Ready {
                    ok: false,
                    version,
                    error: Some(format!("Failed to load model: {e}")),
                };
                let _ = writeln!(io::stdout(), "{}", serde_json::to_string(&ready).unwrap());
                let _ = io::stdout().flush();
                return 3;
            }
        };

        let ready = WorkerLine::Ready {
            ok: true,
            version,
            error: None,
        };
        let _ = writeln!(io::stdout(), "{}", serde_json::to_string(&ready).unwrap());
        let _ = io::stdout().flush();

        let stdin = io::stdin();
        let mut input = stdin.lock();
        let mut buf = String::new();
        loop {
            buf.clear();
            let read = match input.read_line(&mut buf) {
                Ok(n) => n,
                Err(_) => break,
            };
            if read == 0 {
                break;
            }
            let trimmed = buf.trim_end();
            if trimmed.is_empty() {
                continue;
            }

            let msg: WorkerLine = match serde_json::from_str(trimmed) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("[model-worker] invalid request JSON: {e}");
                    continue;
                }
            };

            let (id, system_prompt, prompt, max_tokens) = match msg {
                WorkerLine::Request {
                    id,
                    system_prompt,
                    prompt,
                    max_tokens,
                } => (id, system_prompt, prompt, max_tokens),
                _ => continue,
            };

            let max_tokens = if max_tokens == 0 {
                default_max_tokens.max(16)
            } else {
                max_tokens.max(16)
            };

            let reply =
                match direct_chat_completion(&model, &backend, &system_prompt, &prompt, max_tokens)
                {
                    Ok(text) => WorkerLine::Response {
                        id,
                        ok: true,
                        text: Some(text),
                        error: None,
                    },
                    Err(err) => WorkerLine::Response {
                        id,
                        ok: false,
                        text: None,
                        error: Some(err),
                    },
                };

            if let Ok(json) = serde_json::to_string(&reply) {
                let _ = writeln!(io::stdout(), "{json}");
                let _ = io::stdout().flush();
            }
        }

        0
    }
}

#[cfg(feature = "local-model")]
pub use with_local_model::{
    chat_completion, chat_completion_with_system_prompt, clear_cached_model, run_model_worker,
};

#[cfg(not(feature = "local-model"))]
pub fn clear_cached_model() {}

#[cfg(not(feature = "local-model"))]
pub fn chat_completion(_cfg: &ModelConfig, _user_input: &str) -> Result<String, String> {
    Err(
        "Local model support is disabled in this build. Rebuild with --features local-model."
            .to_string(),
    )
}

#[cfg(not(feature = "local-model"))]
pub fn chat_completion_with_system_prompt(
    _cfg: &ModelConfig,
    _system_prompt: &str,
    _user_input: &str,
) -> Result<String, String> {
    Err(
        "Local model support is disabled in this build. Rebuild with --features local-model."
            .to_string(),
    )
}

#[cfg(not(feature = "local-model"))]
pub fn run_model_worker(_model_path: &std::path::Path, _default_max_tokens: u32) -> i32 {
    eprintln!("Local model support is disabled in this build.");
    2
}
