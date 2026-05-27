use crate::compilation_pipeline::TargetMode;

pub fn source_declares_entry(source: &str, entry: &str) -> bool {
    source.lines().any(|line| {
        let trimmed = line.trim_start();
        !trimmed.starts_with(';') && trimmed.starts_with(&format!("{entry}:"))
    })
}

pub fn infer_target_mode_for_source(
    source: &str,
    is_stdlib: bool,
    fallback: TargetMode,
) -> TargetMode {
    if is_stdlib {
        TargetMode::Hosted
    } else if source_declares_entry(source, "kmain") {
        TargetMode::Kernel
    } else if source_declares_entry(source, "main") {
        TargetMode::Hosted
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::{infer_target_mode_for_source, source_declares_entry};
    use crate::compilation_pipeline::TargetMode;

    #[test]
    fn hosted_is_default_for_main_programs() {
        let mode =
            infer_target_mode_for_source("main: () -> i32 { return 0 }", false, TargetMode::Kernel);
        assert_eq!(mode, TargetMode::Hosted);
    }

    #[test]
    fn kernel_mode_is_selected_for_kmain_programs() {
        let mode =
            infer_target_mode_for_source("kmain: () -> () { return }", false, TargetMode::Hosted);
        assert_eq!(mode, TargetMode::Kernel);
    }

    #[test]
    fn stdlib_always_uses_hosted_mode() {
        let mode =
            infer_target_mode_for_source("kmain: () -> () { return }", true, TargetMode::Kernel);
        assert_eq!(mode, TargetMode::Hosted);
    }

    #[test]
    fn comments_do_not_trigger_detection() {
        assert!(!source_declares_entry(
            "; kmain: () -> () { return }",
            "kmain"
        ));
        assert!(!source_declares_entry(
            "; main: () -> i32 { return 0 }",
            "main"
        ));
    }
}
