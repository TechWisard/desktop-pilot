# Protecting cloud files when Windows disguises their attributes

## Confirmed acceptance finding

The real Windows 11 Cloud Files fixture in run 42270953139 successfully registered its isolated sync root and created a dehydrated Remote.url through CfCreatePlaceholders, but ordinary metadata did not expose FILE_ATTRIBUTE_REPARSE_POINT. The initial acceptance assertion failed. This is not evidence that the Cloud Files API was unavailable.

Microsoft documents that placeholder compatibility can hide reparse, offline and sparse characteristics, making a cloud placeholder look like an ordinary file. The existing organizer checked those attributes before opening content but did not explicitly request their exposure. Consequently the attribute check alone could not guarantee rejection before a content-access attempt in disguised mode. No actual user file was involved or lost.

## Correction

A small thread-bound RAII guard requests PHCM_EXPOSE_PLACEHOLDERS through the documented RtlSetThreadPlaceholderCompatibilityMode API for the duration of a file inspection, identity check or rename preflight. It restores the caller's previous mode on every return, including errors and nested stamp calls. It cannot be sent to a different thread. The API is resolved once from the already loaded ntdll; no new static import is required on Windows predating Cloud Files. A failure to set the mode rejects the operation. This does not change global Windows compatibility, cloud account settings, hydration policy, network access or any other process.

Content access is still preceded by the original protected-attribute check. Before rename, metadata is checked before acquiring content access. Existing file-identity, bytes, destination-handle, no-overwrite and recovery guards remain in force.

## Regression

The explicit disposable Windows 11 test creates an actual 128-byte dehydrated placeholder with no connected provider and no supplied file data. It checks protection in both DISGUISE and EXPOSE modes, requires the protected-object reason (not an error after attempting a cloud read), and verifies restoration of the thread's original mode. With disguised mode active, two local shortcuts are analyzed, moved, the SQLite profile is reopened, and Undo restores their identities. A manual Always shortcut remains untouched. The cloud object's metadata and identity remain unchanged.

The same test verifies actual Desktop Known Folder redirection and restores it, comparing directory identity rather than an 8.3 TEMP spelling. It does not claim synchronization with a real OneDrive account. No physical power-loss or hardware GPU claim is made.

## Primary references

- https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/nf-ntifs-rtlsetthreadplaceholdercompatibilitymode
- https://learn.microsoft.com/en-us/windows/win32/cfapi/build-a-cloud-file-sync-engine (Compatibility with applications that use reparse points)
- https://learn.microsoft.com/en-us/windows/win32/api/cfapi/nf-cfapi-cfcreateplaceholders
- https://github.com/microsoft/win32metadata/blob/main/generation/WinSDK/RecompiledIdlHeaders/um/cfapi.h

The acceptance finding is confirmed; final correctness requires the corrected fixture and the complete integrated CI to pass. Synthetic acceptance is not measured user time savings.
