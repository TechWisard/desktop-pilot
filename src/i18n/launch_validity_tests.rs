use super::translate;

#[test]
fn missing_launch_target_explains_recovery_in_both_languages() {
    let russian =
        "Цель ярлыка или рабочая папка недоступны. Восстановите приложение или подключите диск.";
    let english = "The shortcut target or working folder is unavailable. Restore the app or reconnect the drive.";
    assert_eq!(translate(russian, false), english);
    assert_eq!(translate(english, true), russian);
    assert_eq!(translate(english, false), english);
    assert_eq!(translate(russian, true), russian);
}
