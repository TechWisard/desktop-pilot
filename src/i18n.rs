use std::sync::atomic::{AtomicBool, Ordering};
static RUSSIAN: AtomicBool = AtomicBool::new(false);
pub fn set(language: &str) {
    RUSSIAN.store(language == "ru", Ordering::Relaxed);
}
pub fn text<'a>(ru: &'a str, en: &'a str) -> &'a str {
    if RUSSIAN.load(Ordering::Relaxed) {
        ru
    } else {
        en
    }
}
pub fn runtime(value: &str) -> String {
    translate(value, RUSSIAN.load(Ordering::Relaxed))
}
fn translate(value: &str, russian: bool) -> String {
    let mut output = value.to_owned();
    for (ru, en) in [
        ("Число мест: от 1 до 8", "Slots must be between 1 and 8"),
        (
            "Период: от 1 до 90 дней",
            "Review interval must be between 1 and 90 days",
        ),
        (
            "Память: от 3 до 90 дней",
            "Half-life must be between 3 and 90 days",
        ),
        (
            "Нужно от 1 до 14 дней наблюдений",
            "Required active days must be between 1 and 14",
        ),
        (
            "Порог преимущества: 1,1–3",
            "Advantage must be between 1.1 and 3",
        ),
        (
            "Число замен превышает число мест",
            "Replacement limit exceeds the number of slots",
        ),
        (
            "Порог бездействия: 30–900 секунд",
            "Idle threshold must be between 30 and 900 seconds",
        ),
        ("Недопустимая область размещения", "Invalid shortcut area"),
        (
            "Изменилась геометрия экрана. Примите текущую геометрию в настройках; существующие значки не перемещались.",
            "Display geometry changed. Accept the current geometry in Settings; existing icons were not moved.",
        ),
        (
            "Отказ: путь вне управляемого рабочего стола",
            "Refused: path is outside the managed desktop",
        ),
        (
            "Управлять можно только файлами .lnk",
            "Only .lnk files can be managed",
        ),
        (
            "Отказ: путь является ссылкой файловой системы или каталогом",
            "Refused: path is a filesystem link or directory",
        ),
        (
            "Проводник ещё не завершил обновление списка значков",
            "Explorer has not finished refreshing the icon list",
        ),
        (
            "Слишком большой каталог ярлыков",
            "Shortcut catalog exceeds the supported size",
        ),
        ("Переименование принято:", "Rename accepted:"),
        ("Ручное удаление/перенос:", "Manually deleted or moved:"),
        ("исключён из возврата", "will not be added back"),
        (
            "Изменённый пользователем ярлык оставлен без управления",
            "User-edited shortcut is no longer managed",
        ),
        (
            "Исходный ярлык недоступен",
            "Original shortcut is unavailable",
        ),
        (
            "Место занято одноимённым ярлыком:",
            "A shortcut with this name already exists:",
        ),
        (
            "Ярлык изменился во время операции",
            "Shortcut changed during the operation",
        ),
        (
            "Запись ярлыка не подтверждена",
            "Shortcut write could not be verified",
        ),
        ("Изменение прервано:", "Change interrupted:"),
        ("Автоматический откат:", "Automatic undo:"),
        ("Повреждена резервная копия", "Backup is corrupted"),
        ("Некорректная команда", "Invalid command"),
        ("Неизвестное правило", "Unknown rule"),
        ("Неизвестная команда", "Unknown command"),
        ("Нет URL в ярлыке", "Shortcut contains no URL"),
        (
            "Windows не смогла открыть исходный ярлык",
            "Windows could not open the original shortcut",
        ),
        (
            "Некорректное число объектов рабочего стола",
            "Invalid desktop item count",
        ),
        ("Недопустимая сетка", "Invalid desktop grid"),
        ("Нет геометрии монитора", "Monitor geometry is unavailable"),
        (
            "В Проводнике включено автоматическое упорядочивание: позиции не меняются",
            "Explorer auto-arrange is enabled: positions are unchanged",
        ),
        (
            "Объект ещё не появился в Проводнике или путь неоднозначен:",
            "Item is not visible in Explorer yet, or its path is ambiguous:",
        ),
        (
            "Потерян объект после позиционирования",
            "Item disappeared after positioning",
        ),
        (
            "Проводник не подтвердил координаты:",
            "Explorer did not confirm the coordinates:",
        ),
        ("Слишком большой Taskband", "Taskband data is too large"),
        ("Повреждён Taskband", "Taskband data is corrupted"),
        ("Неподдерживаемый Taskband", "Unsupported Taskband format"),
        ("Оборван PIDL", "Truncated PIDL"),
        ("Лишние данные PIDL", "Unexpected trailing PIDL data"),
        ("Недопустимый PIDL", "Invalid PIDL"),
        ("Нет данных активности", "Activity data is unavailable"),
        ("Нет активного окна", "No foreground window"),
        (
            "Операция уже выполняется другим экземпляром",
            "Another instance is already performing this operation",
        ),
        (
            "При работающем фоне используйте окно настроек",
            "Use Settings while the background process is running",
        ),
        (
            "Исходный ярлык перемещён или удалён. Откройте настройки и обновите список.",
            "Original shortcut moved or deleted. Open Settings and refresh the catalog.",
        ),
        (
            "Исходный ярлык изменён. Обновите список в настройках перед запуском.",
            "Original shortcut changed. Refresh the catalog in Settings before launching.",
        ),
        ("Обнаружен цикл запуска", "Launch loop detected"),
        ("Цель запуска изменилась", "Launch target changed"),
        ("Язык должен быть en или ru", "Language must be en or ru"),
        (
            "Размер значков не помещается в сетку",
            "Icon size does not fit the desktop grid",
        ),
        (
            "Отключите автоупорядочивание Проводника или выберите только наблюдение.",
            "Turn off Explorer auto-arrange, or choose tracking without automatic changes.",
        ),
        (
            "Уже установлена другая версия. Закройте её перед обновлением.",
            "A different version is already installed. Close it before updating.",
        ),
        (
            "Первоначальная настройка завершена",
            "Initial setup completed",
        ),
        (
            "Сначала завершите первоначальную настройку",
            "Complete initial setup before starting background tracking",
        ),
        ("Фоновый учёт работает", "Background tracking is running"),
        ("Изменения приостановлены", "Changes paused"),
        ("Фоновый учёт остановлен", "Background tracking stopped"),
        (
            "Исходный ярлык или цель недоступны",
            "Original shortcut or target is unavailable",
        ),
        ("Исключено вручную", "Excluded by a rule"),
        (
            "Постоянно: ручной выбор",
            "Always on desktop: manual choice",
        ),
        (
            "Уже закреплено на панели задач",
            "Already pinned to the taskbar",
        ),
        (
            "Пока мало независимых дней наблюдений",
            "Not enough independent days of observations",
        ),
        (
            "Повторяемое использование; есть польза прямого входа",
            "Repeated use; eligible for a direct shortcut",
        ),
        (
            "Команда передана фоновому приложению",
            "Command sent to the background process",
        ),
        (
            "Список ярлыков и закрепления обновлены",
            "Shortcut catalog and taskbar pins refreshed",
        ),
        (
            "Правило сохранено. Нажмите «Применить сейчас» для изменения состава.",
            "Rule saved. Choose Apply now to update the shortcuts.",
        ),
        (
            "Проверено: текущий состав сохранён",
            "Checked: current shortcuts kept",
        ),
        (
            "Предыдущее размещение восстановлено. Автоматика приостановлена.",
            "Previous layout restored. Automatic changes are paused.",
        ),
        ("Нет изменений для отката", "No changes to undo"),
        (
            "Текущая геометрия принята; существующие позиции сохранены",
            "Current display geometry accepted; existing positions kept",
        ),
        (
            "История использования очищена; ярлыки и правила сохранены",
            "Usage history cleared; shortcuts and rules kept",
        ),
        ("Исходные ярлыки сохранены.", "Original shortcuts kept."),
        ("Добавлено:", "Added:"),
        ("убрано быстрых копий:", "quick copies removed:"),
        (
            "Ручных закреплений больше доступных мест. Увеличьте число мест или снимите закрепления.",
            "There are more manual pins than slots. Add slots or unpin an app.",
        ),
        (
            "В выбранной области не хватает свободных мест. Существующие значки не перемещены.",
            "Not enough free space in the selected area. Existing icons were not moved.",
        ),
        (
            "Изменилась геометрия экрана. Нажмите «Принять текущую геометрию».",
            "Display geometry changed. Choose Accept current display geometry.",
        ),
        (
            "Есть незавершённая операция. Сначала нажмите «Откатить».",
            "An operation was interrupted. Use Undo first.",
        ),
        (
            "Автоупорядочивание Проводника включено. Изменения остановлены.",
            "Explorer auto-arrange is enabled. Changes are paused.",
        ),
        (
            "Неизвестная версия Taskband; автоматические изменения приостановлены",
            "Unrecognized taskbar data format. Automatic changes are paused. You can disable taskbar pin filtering in Settings.",
        ),
        (
            "Специальный запуск: включение только вручную",
            "Special launcher: manual opt-in only",
        ),
        (
            "Пользователь изменил файл; откат остановлен без перезаписи:",
            "The file was edited; undo stopped without overwriting it:",
        ),
        ("Ручное положение закреплено:", "Manual position pinned:"),
        (
            "Ярлык изменился: удаление отменено",
            "The shortcut changed; removal cancelled",
        ),
        ("Откат", "Undo"),
        ("автоматика приостановлена", "automatic changes paused"),
        (
            "Сначала завершите фоновое приложение",
            "Stop the background process first",
        ),
    ] {
        output = if russian {
            output.replace(en, ru)
        } else {
            output.replace(ru, en)
        };
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn statuses_and_validation_translate_both_ways() {
        assert_eq!(
            translate("Фоновый учёт работает", false),
            "Background tracking is running"
        );
        assert_eq!(
            translate("Initial setup completed", true),
            "Первоначальная настройка завершена"
        );
        assert_eq!(
            translate("Число мест: от 1 до 8", false),
            "Slots must be between 1 and 8"
        );
        assert_eq!(translate("Sample app", false), "Sample app");
    }
}
