#!/bin/bash
# Build dsvita.apk: libdsvita.so (via android_build.sh) + DSVitaActivity, packaged
# without gradle/AGP — those need x86_64 host prebuilts (aapt2 from maven), useless on
# this aarch64 box. Instead: javac + r8/D8 + Ubuntu's native arm64 aapt2/zipalign +
# apksigner, all living under DSVITA_ANDROID_TOOLS (see DEVELOPMENT.md).
#
#   tools/android_apk.sh [profile]     profile: dev (default) | release | release-debug
#
# Layout expected under $DSVITA_ANDROID_TOOLS (default ~/android):
#   jdk/                Temurin JDK (javac/java/keytool)
#   r8.jar              D8 dexer (maven.google.com com.android.tools:r8)
#   android-11/android.jar   platform android.jar (API 30, platform-30_r03.zip)
#   adb-root/           dpkg-extracted arm64 debs: aapt2, zipalign, apksigner + android-lib*
set -e
. "$(dirname "$0")/env.sh"
require_env DSVITA_ANDROID_NDK

: "${DSVITA_ANDROID_TOOLS:=$HOME/android}"
JDK="$DSVITA_ANDROID_TOOLS/jdk"
R8_JAR="$DSVITA_ANDROID_TOOLS/r8.jar"
ANDROID_JAR="$DSVITA_ANDROID_TOOLS/android-11/android.jar"
TOOLS_ROOT="$DSVITA_ANDROID_TOOLS/adb-root"
export LD_LIBRARY_PATH="$TOOLS_ROOT/usr/lib:$TOOLS_ROOT/usr/lib/aarch64-linux-gnu:$TOOLS_ROOT/usr/lib/aarch64-linux-gnu/android:$LD_LIBRARY_PATH"
KEYSTORE="$DSVITA_ANDROID_TOOLS/debug.keystore"

PROFILE="${1:-dev}"

"$DSVITA_TOOLS_DIR/android_build.sh" "$PROFILE"

cd "$DSVITA_ROOT/android"
BUILD="$DSVITA_ROOT/target/android-apk"
rm -rf "$BUILD"
mkdir -p "$BUILD/classes" "$BUILD/dex" "$BUILD/stage/lib/arm64-v8a"

# android.jar as plain classpath (bootclasspath clashes with -source 9+); D8 does the
# API/desugaring work anyway.
"$JDK/bin/javac" -source 17 -target 17 -cp "$ANDROID_JAR" -d "$BUILD/classes" -Xlint:-options java/com/grarak/dsvita/DSVitaActivity.java

"$JDK/bin/java" -cp "$R8_JAR" com.android.tools.r8.D8 --release --lib "$ANDROID_JAR" --min-api 30 --output "$BUILD/dex" "$BUILD/classes/com/grarak/dsvita/"*.class

"$TOOLS_ROOT/usr/bin/aapt2" link -o "$BUILD/unsigned.apk" --manifest AndroidManifest.xml -I "$ANDROID_JAR"

cp "$BUILD/dex/classes.dex" "$BUILD/stage/"
cp app/src/main/jniLibs/arm64-v8a/libdsvita.so "$BUILD/stage/lib/arm64-v8a/"
(cd "$BUILD/stage" && zip -q -r "$BUILD/unsigned.apk" classes.dex lib)

"$TOOLS_ROOT/usr/bin/zipalign" -f 4 "$BUILD/unsigned.apk" "$BUILD/aligned.apk"

if [ ! -f "$KEYSTORE" ]; then
    "$JDK/bin/keytool" -genkeypair -keystore "$KEYSTORE" -alias androiddebugkey -storepass android -keypass android -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=Android Debug,O=Android,C=US"
fi
"$JDK/bin/java" -jar "$TOOLS_ROOT/usr/share/java/apksigner.jar" sign --ks "$KEYSTORE" --ks-pass pass:android --key-pass pass:android --out "$BUILD/dsvita.apk" "$BUILD/aligned.apk"

echo "apk: $BUILD/dsvita.apk ($(du -h "$BUILD/dsvita.apk" | cut -f1))"
