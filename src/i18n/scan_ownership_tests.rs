use super::translate;

#[test]
fn concurrent_scan_retry_notice_is_actionable_in_both_languages() {
    let ru = "Состав быстрых ярлыков изменился во время проверки. Обновите список ещё раз; ручные изменения сохранены.";
    let en = "Quick shortcuts changed during the scan. Refresh the list again; manual changes were preserved.";
    assert_eq!(translate(ru, false), en);
    assert_eq!(translate(en, true), ru);
}
