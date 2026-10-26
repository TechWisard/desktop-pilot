use super::translate;

fn verify_notice(en: &str, ru: &str) {
    for path in [
        r"C:\Tools\Undo\Rare tool.lnk",
        r"\\server\team\Избранное\Rare tool.url",
        r"C:\Tools\Original shortcut is unavailable\Undo.url",
    ] {
        let english = format!("{en} {path}");
        let russian = format!("{ru} {path}");
        assert_eq!(translate(&english, true), russian);
        assert_eq!(translate(&russian, false), english);
        assert_eq!(translate(&english, false), english);
        assert_eq!(translate(&russian, true), russian);
    }
}

#[test]
fn protected_offline_warning_translates_without_changing_paths() {
    verify_notice(
        "The manually protected quick shortcut stays in place until you change its rule or remove it. The original source or target is unavailable:",
        "Закреплённый вами быстрый ярлык остаётся на месте, пока вы не измените правило или не удалите его. Исходный ярлык или цель недоступны:",
    );
}

#[test]
fn automatic_offline_warning_translates_without_changing_paths() {
    verify_notice(
        "The existing quick copy is held for up to 72 hours; new copies are not created. Original source or target is temporarily unavailable:",
        "Существующая быстрая копия сохраняется до 72 часов; новые копии не создаются. Исходный ярлык или цель временно недоступны:",
    );
}
