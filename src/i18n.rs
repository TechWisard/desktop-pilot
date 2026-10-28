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
    // Keep instructions in older history entries aligned with the current UI.
    let value = match value {
        "Есть незавершённая операция. Сначала нажмите «Откатить»." => {
            "Есть незавершённая операция. Сначала нажмите «Отменить последнее изменение»."
        }
        "Изменилась геометрия экрана. Нажмите «Принять текущую геометрию»." => {
            "Изменилась геометрия экрана. Нажмите «Принять текущее размещение»."
        }
        other => other,
    };
    let translations = [
        (
            "Состав быстрых ярлыков изменился во время проверки. Обновите список ещё раз; ручные изменения сохранены.",
            "Quick shortcuts changed during the scan. Refresh the list again; manual changes were preserved.",
        ),
        (
            "Цель ярлыка или рабочая папка недоступны. Восстановите приложение или подключите диск.",
            "The shortcut target or working folder is unavailable. Restore the app or reconnect the drive.",
        ),
        (
            "Этот быстрый ярлык был изменён. Добавьте его исходный ярлык вместо быстрой копии.",
            "This quick shortcut was changed. Add its original shortcut instead.",
        ),
        (
            "Закреплённый вами быстрый ярлык остаётся на месте, пока вы не измените правило или не удалите его. Исходный ярлык или цель недоступны:",
            "The manually protected quick shortcut stays in place until you change its rule or remove it. The original source or target is unavailable:",
        ),
        (
            "Существующая быстрая копия сохраняется до 72 часов; новые копии не создаются. Исходный ярлык или цель временно недоступны:",
            "The existing quick copy is held for up to 72 hours; new copies are not created. Original source or target is temporarily unavailable:",
        ),
        (
            "Не удалось создать ярлык в меню «Пуск». Чтобы открыть настройки, снова запустите Desktop Pilot.",
            "The Start menu shortcut could not be created. Open Settings by launching Desktop Pilot again.",
        ),
        (
            "Не удалось обновить исходный ярлык; сохранено последнее проверенное состояние:",
            "Source shortcut could not be refreshed; its last verified state was kept:",
        ),
        (
            "Фоновая работа остановилась до завершения запуска.",
            "Background work stopped before becoming ready.",
        ),
        (
            "Проводник вернул пустой идентификатор элемента",
            "Explorer returned an empty item identifier",
        ),
        (
            "Команда отменена, поскольку фоновая работа отключена.",
            "Command cancelled because background work was disabled.",
        ),
        (
            "Этому приложению соответствуют несколько сохранённых ярлыков. Обновите список перед повторным добавлением.",
            "Several saved shortcuts match this app. Refresh the list before importing it again.",
        ),
        (
            "Файл отчёта доступен только для чтения. Выберите место, где разрешена запись",
            "The report file is read-only; choose a writable destination",
        ),
        (
            "Действие выполнено, но отчёт сохранить не удалось. Не повторяйте действие только ради экспорта отчёта.",
            "The action completed, but its report could not be saved. Do not repeat the action just to export a report.",
        ),
        ("Укажите полный путь к ярлыку:", "Use a full shortcut path:"),
        (
            "Выберите ярлык .lnk или .url:",
            "Choose a .lnk or .url shortcut:",
        ),
        (
            "Выберите ярлык или вставьте его полный путь.",
            "Choose a shortcut or paste its full path.",
        ),
        ("Добавляем ярлыки…", "Adding shortcuts…"),
        ("Открываем Desktop Pilot…", "Opening Desktop Pilot…"),
        ("Останавливаем фоновую работу…", "Stopping background work…"),
        ("Добавлено ярлыков:", "Shortcuts added:"),
        (
            "Прежние правила и история сохранены.",
            "Existing rules and history were kept.",
        ),
        (
            "Некоторые ярлыки не удалось добавить. Проверьте пути в диалоге и попробуйте ещё раз.",
            "Some shortcuts could not be added. Check the paths in the dialog and try again.",
        ),
        (
            "Desktop Pilot отключён. Ярлыки и локальные данные сохранены. Чтобы вернуться к работе, выберите режим в настройках.",
            "Desktop Pilot is turned off. Your shortcuts and local data are kept. Choose a mode in Settings to use it again.",
        ),
        (
            "Настройки сохранены, но фоновая работа не запустилась:",
            "Settings were saved, but background work could not start:",
        ),
        (
            "Завершите текущий шаг или операцию, затем перетащите ярлыки ещё раз.",
            "Finish the current step or operation, then drop the shortcuts again.",
        ),
        (
            "Значок в области уведомлений временно недоступен. Фоновый учёт продолжается; настройки можно открыть через меню «Пуск».",
            "The tray icon is temporarily unavailable. Background tracking continues; you can open Settings from the Start menu.",
        ),
        (
            "Установлена более новая версия Desktop Pilot. Откройте её через меню «Пуск».",
            "A newer Desktop Pilot version is already installed. Open the installed version from the Start menu.",
        ),
        (
            "В загруженном файле нет корректной информации о версии",
            "The downloaded executable has no valid version information",
        ),
        (
            "Не удалось показать ярлык в Проводнике Windows",
            "Cannot show the shortcut in Windows Explorer",
        ),
        (
            "Не удалось найти ярлык в Проводнике Windows",
            "Cannot locate the shortcut in Windows Explorer",
        ),
        (
            "После неудачного обновления не удалось восстановить фоновую работу:",
            "Background work could not be restored after the failed update:",
        ),
        (
            "Фоновая работа и автозапуск в Windows отключены",
            "Background work and Windows startup disabled",
        ),
        (
            "Команда отменена после завершения предыдущего фонового процесса. Проверьте текущее состояние и при необходимости повторите действие.",
            "The command was cancelled after the previous background process ended. Check the current state and repeat the action if needed.",
        ),
        (
            "Открыто окно настроек другой версии. Закройте его и снова откройте эту версию для обновления.",
            "Another version of Settings is open. Close that window, then open this version again to update.",
        ),
        (
            "Настройки уже открываются. Повторите попытку через несколько секунд.",
            "Settings is already opening. Try again in a moment.",
        ),
        (
            "Фоновая работа запускается или останавливается. Немного подождите и повторите попытку.",
            "Background work is starting or stopping. Wait a moment and try again.",
        ),
        (
            "Фоновый процесс неожиданно завершился",
            "Background process ended unexpectedly",
        ),
        (
            "Выберите файл для отчёта JSON",
            "Choose a file for the JSON report",
        ),
        (
            "Выберите отдельный файл отчёта JSON: файлы программы и базы данных нельзя перезаписывать",
            "Choose a separate JSON report file; application files and databases cannot be overwritten",
        ),
        (
            "Для отчёта нужен обычный файл, а не папка или файловая ссылка",
            "The report destination must be an ordinary file, not a folder or filesystem link",
        ),
        ("Недопустимая папка отчёта", "Invalid report directory"),
        (
            "Не удалось создать временный файл отчёта",
            "Cannot reserve a temporary report file",
        ),
        (
            "Есть незавершённая операция. Сначала нажмите «Отменить последнее изменение».",
            "An operation was interrupted. Choose Undo last change first.",
        ),
        (
            "Изменилась геометрия экрана. Нажмите «Принять текущее размещение».",
            "Display geometry changed. Choose Accept current layout.",
        ),
        (
            "Изменилась геометрия экрана. Нажмите «Принять текущее размещение» в настройках; существующие значки не перемещались.",
            "Display geometry changed. Choose Accept current layout in Settings; existing icons were not moved.",
        ),
        (
            "Запуск фоновой работы ещё не завершён. Проверьте состояние на главной.",
            "Background startup has not completed yet. Check the status in Settings.",
        ),
        (
            "Результат команды больше недоступен. Проверьте текущее состояние на главной.",
            "The command result is no longer available. Check the current state on Home.",
        ),
        (
            "В этой папке находится база данных, не принадлежащая Desktop Pilot. Она не изменена.",
            "This folder contains a database that is not a Desktop Pilot profile; it was not changed",
        ),
        (
            "Не удалось запустить фоновую работу Desktop Pilot",
            "Cannot start Desktop Pilot background work",
        ),
        (
            "Файловая система рабочего стола не позволяет проверить принадлежность ярлыка; размещение отменено",
            "The desktop filesystem cannot verify shortcut ownership; placement was cancelled",
        ),
        (
            "В этом профиле Desktop Pilot нет записи ярлыка. Откройте исходный ярлык или обновите список в настройках.",
            "Shortcut is missing from this Desktop Pilot profile. Open the original shortcut or refresh the list in Settings.",
        ),
        (
            "Управлять можно только ярлыками .lnk и .url",
            "Only .lnk and .url shortcuts can be managed",
        ),
        (
            "Наблюдение ещё идёт. Ярлыки не изменены.",
            "Passive observation is still in progress. No shortcuts were changed.",
        ),
        (
            "Число сеансов обучения: от 1 до 100",
            "Learning sessions must be between 1 and 100",
        ),
        (
            "Число дней обучения: от 1 до 90",
            "Learning days must be between 1 and 90",
        ),
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
            "Пока мало активности для нового быстрого ярлыка",
            "Not enough usage yet for a new quick shortcut",
        ),
        (
            "Повторяемое использование; есть польза прямого входа",
            "Repeated use; eligible for a direct shortcut",
        ),
        (
            "Сохранённый исторический приоритет; свежих наблюдений пока мало",
            "Saved historical priority; little recent activity has been observed",
        ),
        (
            "Продолжительная работа за день; быстрый ярлык может быть полезен",
            "Extended use in one day; a quick shortcut may help",
        ),
        (
            "Несколько сеансов за день; быстрый ярлык может быть полезен",
            "Several sessions in one day; a quick shortcut may help",
        ),
        (
            "Использование в разные дни; быстрый ярлык может быть полезен",
            "Usage on different days; a quick shortcut may help",
        ),
        (
            "Наблюдалась работа с приложением; быстрый ярлык может быть полезен",
            "Activity observed; a quick shortcut may help",
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
        ("Передаём команду…", "Sending the command…"),
        ("Запускаем фоновую работу…", "Starting background work…"),
        ("Сохраняем настройки…", "Saving settings…"),
        ("Подготавливаем Desktop Pilot…", "Preparing Desktop Pilot…"),
        ("Добавляем ярлык…", "Adding the shortcut…"),
        ("Настройки сохранены.", "Settings saved."),
        (
            "Несохранённые правки отменены.",
            "Unsaved changes discarded.",
        ),
        ("Фоновая работа запущена.", "Background work started."),
        (
            "Ярлык добавлен. Прежние правила и история сохранены.",
            "Shortcut added. Existing rules and history were kept.",
        ),
        (
            "Desktop Pilot готов. Окно можно закрыть: программа продолжит работу в области уведомлений.",
            "Desktop Pilot is ready. You can close this window; it will keep working in the tray.",
        ),
        (
            "Дождитесь завершения текущей операции перед закрытием.",
            "Please wait for the current operation to finish before closing.",
        ),
        (
            "Операция неожиданно остановилась. Сохранённые настройки доступны; попробуйте снова.",
            "The operation stopped unexpectedly. Your saved settings are available; try again.",
        ),
        (
            "Не удалось прочитать размещение рабочего стола. Попробуйте обновить схему.",
            "Could not read the desktop layout. Try refreshing it.",
        ),
        (
            "Эти настройки изменились в фоне. Отмените правки и повторите нужное изменение.",
            "These settings changed in the background. Discard your edits, then make the change again.",
        ),
        ("Некорректный формат настроек", "Invalid settings format"),
        (
            "Учёт использования приостановлен",
            "Usage tracking is paused",
        ),
        (
            "Настройка автоматики изменена через значок программы",
            "Automation toggled from the tray",
        ),
        (
            "Настройка учёта активности изменена через значок программы",
            "Activity tracking toggled from the tray",
        ),
        (
            "Не удалось обновить автозапуск:",
            "Autostart could not be updated:",
        ),
        (
            "Не удалось обновить закрепления панели задач:",
            "Taskbar pins could not be refreshed:",
        ),
        (
            "Не удалось проверить меню «Пуск»:",
            "Start menu could not be scanned:",
        ),
        (
            "Достигнут предел поиска в 3000 ярлыков; прежние записи сохранены",
            "Shortcut scan limit reached (3000); existing catalog entries were retained",
        ),
        (
            "Достигнут предел поиска в 6000 ярлыков; прежние записи сохранены",
            "Shortcut scan limit reached (6000); existing catalog entries were retained",
        ),
        (
            "Закрепления панели задач не проверены. Обновите список или отключите их учёт в настройках.",
            "Taskbar pins could not be verified. Refresh the list or turn off taskbar pin filtering in Settings.",
        ),
        (
            "Файл ярлыка заменён другим файлом и сохранён без изменений",
            "Shortcut was replaced by another file; it was preserved",
        ),
        (
            "Исходный ярлык выведен из управления:",
            "Original shortcut released from management:",
        ),
        (
            "Не удалось выбрать свободное имя ярлыка",
            "Unable to find a free shortcut filename",
        ),
        (
            "Координаты рабочего стола вышли за допустимый диапазон",
            "Desktop coordinates overflow",
        ),
        ("У ярлыка нет расширения", "Shortcut extension is missing"),
        (
            "Подготовленный ярлык недоступен",
            "Prepared shortcut is missing",
        ),
        (
            "Недопустимый путь временного файла операции",
            "Invalid transaction staging path",
        ),
        (
            "Недопустимый временный файл операции",
            "Invalid transaction staging file",
        ),
        (
            "Недопустимый идентификатор операции",
            "Invalid transaction identifier",
        ),
        (
            "Недопустимое действие в операции",
            "Invalid transaction operation",
        ),
        (
            "В операции обнаружены конфликтующие пути ярлыков",
            "Transaction contains conflicting shortcut paths",
        ),
        (
            "Отмена не может изменять исходный ярлык",
            "An original shortcut cannot be modified by Undo",
        ),
        (
            "Недопустимый путь резервной копии операции",
            "Invalid transaction backup path",
        ),
        (
            "Временный файл отмены изменился и сохранён без перезаписи",
            "Undo staging file was changed; it was preserved",
        ),
        (
            "Временный файл отмены недоступен",
            "Undo staging file is missing",
        ),
        (
            "Команда прервана при завершении приложения. Проверьте текущее состояние и при необходимости повторите действие.",
            "The command was interrupted when the app stopped. Check the current state and repeat the action if needed.",
        ),
        (
            "Недопустимый идентификатор ярлыка",
            "Invalid shortcut identifier",
        ),
        (
            "Некорректный исторический рейтинг ярлыка",
            "Invalid historical shortcut score",
        ),
        (
            "Файл интернет-ярлыка слишком большой",
            "Internet shortcut is too large",
        ),
        (
            "Оборваны данные UTF-16 интернет-ярлыка",
            "Incomplete UTF-16 internet shortcut",
        ),
        (
            "Некорректные данные UTF-16 интернет-ярлыка",
            "Invalid UTF-16 internet shortcut",
        ),
        (
            "Не удалось прочитать текст интернет-ярлыка",
            "Cannot decode internet shortcut",
        ),
        (
            "Недопустимая кодировка интернет-ярлыка",
            "Invalid internet shortcut encoding",
        ),
        (
            "В интернет-ярлыке обнаружен нулевой символ",
            "Internet shortcut contains an embedded NUL",
        ),
        ("Файл ярлыка не найден:", "Shortcut file does not exist:"),
        (
            "Поддерживаются только ярлыки .lnk и .url",
            "Only .lnk and .url shortcuts are supported",
        ),
        (
            "Не удалось прочитать конфигурацию мониторов",
            "Cannot read monitor topology",
        ),
        (
            "Монитор рабочего стола недоступен",
            "No desktop monitor is available",
        ),
        (
            "Не удалось прочитать координаты рабочего стола",
            "Cannot read desktop screen coordinates",
        ),
        (
            "Не удалось прочитать закреплённый ярлык:",
            "Cannot read pinned shortcut:",
        ),
        (
            "Активное окно изменилось во время проверки",
            "Foreground window changed while sampling",
        ),
        (
            "Некорректная команда автозапуска",
            "Invalid startup command",
        ),
        (
            "Не удалось прочитать команду автозапуска",
            "Cannot read startup command",
        ),
        (
            "Недопустимое число аргументов автозапуска",
            "Invalid startup argument count",
        ),
        (
            "Некорректное значение автозапуска в реестре",
            "Invalid startup registry value",
        ),
        (
            "Установленный файл программы недоступен. Переустановите Desktop Pilot перед включением автозапуска",
            "The installed executable is missing; reinstall Desktop Pilot before enabling startup",
        ),
        (
            "Путь установки слишком длинный для автозапуска Windows. Выберите более короткий путь профиля",
            "The installation path is too long for Windows startup; choose a shorter profile path",
        ),
        (
            "Не удалось открыть настройки Desktop Pilot",
            "Cannot open Desktop Pilot settings",
        ),
        (
            "Не удалось обновить значок Desktop Pilot в области уведомлений",
            "Cannot update the Desktop Pilot tray icon",
        ),
        (
            "Не удалось включить управление значком с клавиатуры",
            "Cannot enable keyboard access for the tray icon",
        ),
        (
            "Не удалось зарегистрировать окно области уведомлений Desktop Pilot",
            "Cannot register the Desktop Pilot tray window",
        ),
        (
            "Не удалось зарегистрировать уведомления о перезапуске Проводника",
            "Cannot register Explorer restart notifications",
        ),
        (
            "Не удалось запустить таймер завершения фоновой работы",
            "Cannot start the tray shutdown timer",
        ),
        (
            "Ошибка обработки сообщений области уведомлений Windows",
            "The Windows tray message loop failed",
        ),
        (
            "Не удалось создать значок в области уведомлений",
            "Failed to create the tray icon",
        ),
        (
            "Фоновая работа ещё завершается. Подождите и повторно откройте новую версию.",
            "Background work is still finishing. Wait for it to stop, then open the new version again.",
        ),
        (
            "Установленный файл программы недоступен",
            "The installed executable is missing",
        ),
        (
            "В выбранной папке уже есть другой desktop-pilot.exe. Выберите пустую папку профиля.",
            "A different desktop-pilot.exe already exists in the selected folder. Choose an empty profile folder.",
        ),
        (
            "Подготовленное обновление не прошло проверку",
            "The staged update failed verification",
        ),
        (
            "Установленный файл программы изменился во время обновления",
            "The installed executable changed during the update",
        ),
        (
            "Путь резервной копии программы занят другим файлом",
            "A different file already occupies the executable backup path",
        ),
        (
            "Резервная копия программы не прошла проверку",
            "The executable backup failed verification",
        ),
        (
            "Не удалось заменить установленный файл. Закройте предыдущую версию и повторите попытку.",
            "Cannot replace the installed executable. Close the previous version and try again.",
        ),
        (
            "Установленный файл программы не прошёл проверку",
            "The installed executable failed verification",
        ),
        (
            "Восстановление автозапуска также не удалось:",
            "Startup rollback also failed:",
        ),
        (
            "Фоновое приложение завершилось при запуске:",
            "Background process exited during startup:",
        ),
        (
            "Фоновое приложение не подтвердило запуск. Подробности доступны в истории.",
            "Background process did not confirm startup. Check the history for details.",
        ),
        (
            "Нельзя заменять каталог или ссылку файловой системы:",
            "Refusing to replace a directory or filesystem link:",
        ),
        (
            "Параметр --home можно указать только один раз",
            "--home may only be specified once",
        ),
        ("Не указан путь --home", "Missing --home path"),
        (
            "Не указан или недопустим путь --home",
            "Missing or invalid --home path",
        ),
        (
            "Переменная LOCALAPPDATA недоступна; задайте профиль через --home",
            "LOCALAPPDATA is missing; use --home to choose a profile",
        ),
        ("Недопустимый путь профиля", "Invalid profile path"),
        (
            "Недопустимая родительская папка профиля",
            "Invalid profile parent",
        ),
        (
            "Путь профиля не является каталогом",
            "The profile path is not a directory",
        ),
        (
            "Не удалось определить начало местного дня",
            "Cannot determine the start of the local day",
        ),
    ];
    translate_phrases(value, russian, &translations)
}

// Translate known messages once. Do not reinterpret file paths, URLs or
// fragments inside words as status messages.
fn translate_phrases(value: &str, russian: bool, translations: &[(&str, &str)]) -> String {
    let mut output = String::with_capacity(value.len());
    let mut offset = 0;
    while offset < value.len() {
        let rest = &value[offset..];
        let bytes = rest.as_bytes();
        let path = rest.starts_with(r"\\")
            || (bytes.len() >= 3
                && bytes[0].is_ascii_alphabetic()
                && bytes[1] == b':'
                && matches!(bytes[2], b'\\' | b'/'))
            || rest.starts_with("https://")
            || rest.starts_with("http://");
        if path {
            let length = rest.find('\n').unwrap_or(rest.len());
            output.push_str(&rest[..length]);
            offset += length;
            continue;
        }
        let found = translations
            .iter()
            .filter_map(|(ru, en)| {
                let (source, target) = if russian { (*en, *ru) } else { (*ru, *en) };
                if !rest.starts_with(source) {
                    return None;
                }
                if offset > 0 && !source.contains(' ') && !source.ends_with(':') {
                    return None;
                }
                let protected =
                    |c: char| c.is_alphanumeric() || matches!(c, '_' | '/' | '\\' | '.' | '-');
                if value[..offset].chars().next_back().is_some_and(protected)
                    || rest[source.len()..].chars().next().is_some_and(protected)
                {
                    return None;
                }
                Some((source, target))
            })
            .max_by_key(|(source, _)| source.len());
        if let Some((source, target)) = found {
            output.push_str(target);
            offset += source.len();
        } else {
            let c = rest.chars().next().expect("nonempty remaining message");
            output.push(c);
            offset += c.len_utf8();
        }
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

    #[test]
    fn current_and_historical_scan_limit_messages_translate_both_ways() {
        let messages = [
            (
                "Shortcut scan limit reached (3000); existing catalog entries were retained",
                "Достигнут предел поиска в 3000 ярлыков; прежние записи сохранены",
            ),
            (
                "Shortcut scan limit reached (6000); existing catalog entries were retained",
                "Достигнут предел поиска в 6000 ярлыков; прежние записи сохранены",
            ),
        ];
        for (english, russian) in messages {
            assert_eq!(translate(english, true), russian);
            assert_eq!(translate(russian, false), english);
        }
    }

    #[test]
    fn paths_urls_and_names_inside_messages_are_preserved() {
        for path in [
            r"C:\Tools\Undo\Shortcut.lnk",
            r"\\server\Undo\Shortcut.lnk",
            "https://example.com/Undo",
        ] {
            assert_eq!(translate(path, true), path);
        }
        assert_eq!(translate("Undo.exe", true), "Undo.exe");
        assert_eq!(
            translate("Manual position pinned: Undo", true),
            "Ручное положение закреплено: Undo"
        );
        assert_eq!(
            translate(
                r"Original shortcut is unavailable: C:\Tools\Undo\Original.lnk",
                true
            ),
            r"Исходный ярлык недоступен: C:\Tools\Undo\Original.lnk"
        );
        assert_eq!(
            translate(
                r"Source shortcut could not be refreshed; its last verified state was kept: \\server\Undo\App.lnk: Access denied",
                true
            ),
            r"Не удалось обновить исходный ярлык; сохранено последнее проверенное состояние: \\server\Undo\App.lnk: Access denied"
        );
    }

    #[test]
    fn the_longest_message_wins_without_translating_its_output_again() {
        let phrases = [
            ("Первый", "Ready"),
            ("Готово целиком", "Ready now"),
            ("Ошибочный повтор", "Первый"),
        ];
        assert_eq!(
            translate_phrases("Ready now", true, &phrases),
            "Готово целиком"
        );
        assert_eq!(translate_phrases("Ready", true, &phrases), "Первый");
    }

    #[test]
    fn notices_and_background_warnings_switch_languages() {
        assert_eq!(translate("Settings saved.", true), "Настройки сохранены.");
        assert_eq!(translate("Sending the command…", true), "Передаём команду…");
        assert_eq!(
            translate("Учёт использования приостановлен", false),
            "Usage tracking is paused"
        );
        assert_eq!(
            translate("Autostart could not be updated: access denied", true),
            "Не удалось обновить автозапуск: access denied"
        );
    }
}

#[cfg(test)]
mod offline_tests;

#[cfg(test)]
mod import_ownership_tests;

#[cfg(test)]
mod launch_validity_tests;

#[cfg(test)]
mod cold_start_tests {
    use super::translate;

    #[test]
    fn early_candidate_explains_why_no_shortcut_was_added() {
        let russian = "Пока мало активности для нового быстрого ярлыка";
        let english = "Not enough usage yet for a new quick shortcut";
        assert_eq!(translate(russian, false), english);
        assert_eq!(translate(english, true), russian);
    }
}

#[cfg(test)]
mod scan_ownership_tests;

#[cfg(test)]
mod evidence_reason_translation_tests {
    use super::translate;

    #[test]
    fn usage_evidence_explanations_roundtrip_in_both_languages() {
        for (ru, en) in [
            (
                "Сохранённый исторический приоритет; свежих наблюдений пока мало",
                "Saved historical priority; little recent activity has been observed",
            ),
            (
                "Продолжительная работа за день; быстрый ярлык может быть полезен",
                "Extended use in one day; a quick shortcut may help",
            ),
            (
                "Несколько сеансов за день; быстрый ярлык может быть полезен",
                "Several sessions in one day; a quick shortcut may help",
            ),
            (
                "Использование в разные дни; быстрый ярлык может быть полезен",
                "Usage on different days; a quick shortcut may help",
            ),
            (
                "Наблюдалась работа с приложением; быстрый ярлык может быть полезен",
                "Activity observed; a quick shortcut may help",
            ),
        ] {
            assert_eq!(translate(ru, false), en);
            assert_eq!(translate(en, true), ru);
        }
    }
}
