use crate::local_model;
use crate::settings::{JanetConfig, Settings};

fn no_model_selected_message() -> String {
    "Chatty is not ready yet. Drop a GGUF into data/models/ to get started, or ask a teacher to choose one in File -> Models.".to_string()
}

fn friendly_model_error(err: &str) -> String {
    let lower = err.to_ascii_lowercase();

    if lower.contains("model file not found") || lower.contains("could not resolve model path") {
        return no_model_selected_message();
    }

    if lower.contains("does not look like gguf")
        || lower.contains("could not open model file")
        || lower.contains("could not read model file header")
    {
        return "I couldn't run the local model because the selected file does not look usable. Drop a valid GGUF into data/models/ or ask a teacher to choose one in File -> Models.".to_string();
    }

    if lower.contains("local model support is disabled") {
        return "This build does not have local model support enabled. Rebuild with local-model support to use AI features.".to_string();
    }

    if lower.contains("incompatible")
        || lower.contains("failed to create model context")
        || lower.contains("model worker exited before it became ready")
        || lower.contains("model worker crashed")
    {
        return "I couldn't run the local model because this GGUF appears incompatible with the current build. Try a different model in File -> Models.".to_string();
    }

    "I couldn't run the local model right now. Drop a GGUF into data/models/ to get started, or ask a teacher to check File -> Models.".to_string()
}

pub fn generate_answer(settings: &Settings, user_input: &str) -> String {
    if settings.model.path.trim().is_empty() || settings.model.name == "No model selected" {
        return no_model_selected_message();
    }

    match local_model::chat_completion(&settings.model, user_input) {
        Ok(text) => text,
        Err(err) => friendly_model_error(&err),
    }
}

pub fn generate_answer_with_system_prompt(
    settings: &Settings,
    system_prompt: &str,
    user_input: &str,
) -> String {
    if settings.model.path.trim().is_empty() || settings.model.name == "No model selected" {
        return no_model_selected_message();
    }

    match local_model::chat_completion_with_system_prompt(
        &settings.model,
        system_prompt,
        user_input,
    ) {
        Ok(text) => text,
        Err(err) => friendly_model_error(&err),
    }
}

pub fn janet_filter(janet: &JanetConfig, answer: &str, user_input: &str) -> String {
    if !janet.enabled {
        return answer.to_string();
    }

    let banned_swears = [
        "fuck", "shit", "cunt", "bitch", "bastard", "crap", "piss", "dick", "cock", "tits",
        "asshole", "ass", "bollock",
    ];
    let masked_swears = ["fk", "fck", "fuk", "sht", "sh1t", "btch", "b1tch", "biatch"];
    let banned_mature = ["sex", "porn", "drugs", "suicide", "kill", "terrorist"];

    let normalize = |text: &str| -> String {
        text.to_lowercase()
            .chars()
            .filter_map(|c| match c {
                '0' => Some('o'),
                '1' | '!' | '|' => Some('i'),
                '3' => Some('e'),
                '4' => Some('a'),
                '5' => Some('s'),
                '7' => Some('t'),
                '8' => Some('b'),
                '9' => Some('g'),
                _ if c.is_ascii_alphabetic() => Some(c),
                _ => None, // strip masking like *, -, _
            })
            .collect()
    };
    let drop_vowels = |text: &str| -> String {
        text.chars()
            .filter(|c| !matches!(c, 'a' | 'e' | 'i' | 'o' | 'u'))
            .collect()
    };

    let lower_in = user_input.to_lowercase();
    let lower_ans = answer.to_lowercase();
    let normalized_in = normalize(&lower_in);
    let _normalized_ans = normalize(&lower_ans);
    let vowelless_in = drop_vowels(&normalized_in);

    let contains_swear = janet.block_swears
        && banned_swears.iter().any(|w| {
            let w_vowelless = drop_vowels(w);
            lower_in.contains(w)
                || normalized_in.contains(w)
                || (!w_vowelless.is_empty() && vowelless_in.contains(&w_vowelless))
        });

    let masked_hit = janet.block_swears && masked_swears.iter().any(|w| normalized_in.contains(w));

    let contains_mature =
        janet.block_mature_topics && banned_mature.iter().any(|w| lower_in.contains(w));

    if contains_swear || masked_hit || contains_mature {
        return janet.fallback_message.clone();
    }

    answer.to_string()
}
