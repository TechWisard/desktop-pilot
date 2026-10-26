use super::translate;

#[test]
fn changed_quick_copy_import_explains_recovery_in_both_languages() {
    let english = "This quick shortcut was changed. Add its original shortcut instead.";
    let russian =
        "Этот быстрый ярлык был изменён. Добавьте его исходный ярлык вместо быстрой копии.";
    assert_eq!(translate(english, true), russian);
    assert_eq!(translate(russian, false), english);
    assert_eq!(translate(english, false), english);
    assert_eq!(translate(russian, true), russian);
}
