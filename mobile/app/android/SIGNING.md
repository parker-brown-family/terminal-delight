# Signing

Release builds are signed with the Terminal Delight app key. Always install
release builds: a build signed with a different key cannot update the installed
app, and Android's answer is to uninstall it — which deletes the pairing token.

| | |
|---|---|
| Keystore | `~/Documents/_BACKUPS/terminal-delight/mobile-app/terminal-delight-release.p12` (PKCS12) |
| Alias | `terminaldelight` |
| Passwords | in `key.properties` beside the keystore, copied to the gitignored `android/key.properties` |
| SHA-1 | `A1:5C:04:26:8B:73:4A:52:F9:84:1A:AE:63:19:70:42:93:71:FA:0E` |
| SHA-256 | `56:43:9E:AA:27:56:50:B8:66:EC:D2:72:AD:C9:AD:8A:E9:9A:42:0F:A4:3D:9B:DD:F8:0E:D3:03:CE:9D:D0:0C` |

A release build without `android/key.properties` fails in Gradle rather than
quietly falling back to the debug key — `android/app/build.gradle.kts`.

On a new machine:

```sh
cp ~/Documents/_BACKUPS/terminal-delight/mobile-app/key.properties android/key.properties
```

Check what an APK is signed with:

```sh
/home/parker/Android/Sdk/build-tools/36.0.0/apksigner verify --print-certs build/app/outputs/flutter-apk/app-release.apk
```
