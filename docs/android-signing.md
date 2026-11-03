# Signing Android releases in CI

The release workflow (`.github/workflows/release.yml`) signs the Android APK
and AAB with your release key when the repository secrets below exist. Without
them it falls back to the debug key: fine for sideloading, not for the Play
Store.

The keystore lives at `src-tauri/keystore/Kamune-ORG.jks`. That directory is
git-ignored, so the key is never committed.

## Creating a new keystore

Only do this if the app has never been distributed with the current key, or if
you accept that existing installs can't update to builds signed with the new
one. On Google Play with Play App Signing, this key is the *upload key* and can
be reset from the Play Console instead.

You need `keytool` from any JDK 17 or newer (Android Studio bundles one).

1. Create the password in your password manager first (for example, a
   32-character random password in Bitwarden) and save the item.
2. Move the old keystore out of the way (don't delete it until you are sure
   it's not needed):

   ```sh
   mv src-tauri/keystore/Kamune-ORG.jks src-tauri/keystore/Kamune-ORG.old.jks
   ```

3. Generate the new key. `keytool` asks for the password twice; paste it from
   the password manager (input is not echoed):

   ```sh
   keytool -genkeypair -keystore src-tauri/keystore/Kamune-ORG.jks \
     -storetype PKCS12 -alias stegano \
     -keyalg RSA -keysize 4096 -validity 10000 \
     -dname "CN=Hossein Yazdani, O=Kamune"
   ```

   This is a PKCS#12 keystore, so the key password is the same as the store
   password; only one password exists. The alias `stegano` is what the build
   expects by default, and 10,000 days (about 27 years) meets Google Play's
   requirement that keys stay valid past 2033.

4. Check it (prompts for the password):

   ```sh
   keytool -list -v -keystore src-tauri/keystore/Kamune-ORG.jks
   ```

5. Back it up next to the password, for example as an attachment on the same
   Bitwarden item, or as text in a secure note (the base64 form fits within
   Bitwarden's note limit):

   ```sh
   base64 -i src-tauri/keystore/Kamune-ORG.jks | pbcopy
   ```

Then set the CI secrets below. With this keystore only
`ANDROID_KEYSTORE_BASE64` and `ANDROID_KEYSTORE_PASSWORD` are needed.

## One-time setup

Run these from the repository root. `gh` prompts for each value without
echoing it, so the passwords never land in your shell history.

1. Upload the keystore, base64-encoded:

   ```sh
   base64 -i src-tauri/keystore/Kamune-ORG.jks | gh secret set ANDROID_KEYSTORE_BASE64 -R kamune-org/stegano
   ```

2. Store password:

   ```sh
   gh secret set ANDROID_KEYSTORE_PASSWORD -R kamune-org/stegano
   ```

3. Key alias. This is needed unless the alias is `stegano`, which the build
   assumes otherwise. To look it up (needs a JDK and the store password):

   ```sh
   keytool -list -keystore src-tauri/keystore/Kamune-ORG.jks
   gh secret set ANDROID_KEY_ALIAS -R kamune-org/stegano
   ```

4. Key password, only if it differs from the store password:

   ```sh
   gh secret set ANDROID_KEY_PASSWORD -R kamune-org/stegano
   ```

Check what is set with `gh secret list -R kamune-org/stegano`.

## What happens on a release

On every pushed `v*` tag, the Android job:

1. decodes `ANDROID_KEYSTORE_BASE64` into a temporary file on the runner;
2. exports `MY_STORE_FILE`, `MY_STORE_PASSWORD`, `MY_KEY_ALIAS` and
   `MY_KEY_PASSWORD`, which `src-tauri/gen/android/app/build.gradle.kts` reads;
3. builds and signs the APK and AAB and attaches them to the draft release.

GitHub masks secret values in the logs. The same environment variables work
for local release builds, if you'd rather not keep the default path.

## Recommendations

- **Back up the `.jks` somewhere other than this machine**, such as a password
  manager or an encrypted drive. If it is lost, updates can no longer be
  published under the same app identity.
- **Optional:** put these secrets in a GitHub Environment (for example
  `release`) restricted to `v*` tags, optionally with required reviewers, and
  add `environment: release` to the Android job. Then only that job can read
  the signing key.
