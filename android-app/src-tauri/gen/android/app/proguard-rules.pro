# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# LocalVault：Rust 侧通过 JNI 按类名/方法名调用 MainActivity 的静态方法（指纹/分享），
# release 版 R8 混淆会重命名这些符号导致调用失败（"Java exception was raised during method invocation"）。
# 必须保留类名与全部成员名。
-keep class com.localvault.mobile.MainActivity { *; }

# Rust 侧 webview 方法（clear_all_browsing_data 等）经 JNI 调用 RustWebView 的 Kotlin 方法，
# R8 混淆会重命名（曾导致 NoSuchMethodError: RustWebView.clearAllBrowsingData 闪退），必须保留。
-keep class com.localvault.mobile.RustWebView { *; }
-keep class com.localvault.mobile.RustWebViewClient { *; }
-keep class com.localvault.mobile.Rust { *; }
-keep class com.localvault.mobile.Ipc { *; }

# If your project uses WebView with JS, uncomment the following
# and specify the fully qualified class name to the JavaScript interface
# class:
#-keepclassmembers class fqcn.of.javascript.interface.for.webview {
#   public *;
#}

# Uncomment this to preserve the line number information for
# debugging stack traces.
#-keepattributes SourceFile,LineNumberTable

# If you keep the line number information, uncomment this to
# hide the original source file name.
#-renamesourcefileattribute SourceFile