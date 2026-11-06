from pathlib import Path

def replace(path, old, new, count=1):
    target = Path(path)
    text = target.read_text()
    assert text.count(old) == count, (path, old, text.count(old))
    target.write_text(text.replace(old, new))

replace('src/organize/mod.rs', 'mod journal;', 'mod journal;\nmod refresh;\npub(crate) mod runtime_checks;')
replace('src/organize/journal.rs', '    native::refresh(Path::new(&journal.root));\n    let current = native::layout()?;', '    let current = super::refresh::settled(journal, undo, restored)?;')
# These scenario tests already provide an idle snapshot. Their second boundary
# must use the existing test-only input seam, not the hosted machine's keyboard.
path = Path('src/engine.rs')
text = path.read_text()
start = text.index('fn a_future_review_checkpoint_does_not_freeze_automatic_recovery()')
end = text.index('fn sqlite_writer_lock_retries_the_real_usage_transaction_once()', start)
piece = text[start:end]
assert piece.count('automatic_review(store, true)') == 7
text = text[:start] + piece.replace('automatic_review(store, true)', 'automatic_review_with_boundary(store, true, || {})') + text[end:]
path.write_text(text)
path = Path('src/model/cold_start_tests.rs')
text = path.read_text()
assert text.count('engine::automatic_review(store, true)') == 5
path.write_text(text.replace('engine::automatic_review(store, true)', 'engine::automatic_review_with_boundary(store, true, || {})'))
replace('src/main.rs', '    engine::rollback(&protocol)?;\n    emit(', '    engine::rollback(&protocol)?;\n    organize::runtime_checks::run(home)?;\n    emit(')
replace('src/main.rs', '                "failed executable replacement restores the previous worker and profile",', '                "failed executable replacement restores the previous worker and profile",\n                "organizer stops and resumes the actual worker around Apply and Undo",\n                "organizer restart preserves newer manual settings and Turn off",')
