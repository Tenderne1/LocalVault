package com.localvault.mobile

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import androidx.activity.enableEdgeToEdge
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import java.io.File
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    // 关键：清 WebView 缓存目录——tauri.localhost 的 index.html 会被 WebView 缓存，
    // 覆盖安装（install -r）后缓存目录不清，旧 index.html 引用旧 JS 导致界面不更新。
    // 每次启动先删缓存，强制从 APK assets 加载最新前端。
    try {
      cacheDir.deleteRecursively()
    } catch (_: Exception) { }
    super.onCreate(savedInstanceState)
    instance = this
    // 禁用 WebView 磁盘缓存：更新安装后强制从 APK assets 加载最新前端，避免旧界面残留
    try {
      findWebView(window.decorView)?.settings?.cacheMode = 2 // WebView.LOAD_NO_CACHE
    } catch (_: Exception) { }
  }

  /**
   * 返回键桥：不执行默认行为（退出/后退），而是把事件交给前端 JS 处理。
   * 前端按状态消费：多选→取消选择 / 弹窗→关闭 / 详情/编辑→返回列表 / 列表页→两次返回退出。
   * 前端确认退出时调用 mobile_exit → finishActivity()。webview 未就绪时才走默认。
   */
  @Suppress("DEPRECATION")
  override fun onBackPressed() {
    val wv = findWebView(window.decorView)
    if (wv != null) {
      wv.post { wv.evaluateJavascript("window.__lvHandleBack ? window.__lvHandleBack() : null", null) }
      return
    }
    super.onBackPressed()
  }

  private fun findWebView(v: android.view.View): android.webkit.WebView? {
    if (v is android.webkit.WebView) return v
    if (v is android.view.ViewGroup) {
      for (i in 0 until v.childCount) {
        findWebView(v.getChildAt(i))?.let { return it }
      }
    }
    return null
  }

  companion object {
    @Volatile var instance: MainActivity? = null
    // -1 等待中 / 0 失败或取消 / 1 成功
    @Volatile var bioResult: Int = -1
    private val mainHandler = Handler(Looper.getMainLooper())
    private const val BIO_KS_ALIAS = "localvault_bio_ks"

    /** 前端确认退出：结束 Activity（配合两次返回确认） */
    @JvmStatic
    fun finishActivity() {
      mainHandler.post {
        val act = instance ?: return@post
        try {
          act.finish()
          act.finishAndRemoveTask()
        } catch (_: Exception) { }
      }
    }

    @JvmStatic
    fun bioAvailable(): Boolean {
      val act = instance ?: return false
      val bm = BiometricManager.from(act)
      return bm.canAuthenticate(BiometricManager.Authenticators.BIOMETRIC_STRONG) == BiometricManager.BIOMETRIC_SUCCESS
    }

    @JvmStatic
    fun startBiometricAuth() {
      bioResult = -1
      mainHandler.post {
        val act = instance
        if (act == null || !bioAvailable()) { bioResult = 0; return@post }
        val prompt = BiometricPrompt(
          act as FragmentActivity,
          ContextCompat.getMainExecutor(act),
          object : BiometricPrompt.AuthenticationCallback() {
            override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) { bioResult = 1 }
            override fun onAuthenticationError(errorCode: Int, errString: CharSequence) { bioResult = 0 }
            override fun onAuthenticationFailed() { bioResult = 0 }
          }
        )
        prompt.authenticate(
          BiometricPrompt.PromptInfo.Builder()
            .setTitle("指纹解锁 LocalVault")
            .setSubtitle("使用已录入的指纹解锁手机端密码库")
            .setNegativeButtonText("取消")
            .setAllowedAuthenticators(BiometricManager.Authenticators.BIOMETRIC_STRONG)
            .build()
        )
      }
    }

    @JvmStatic
    fun shareText(text: String, subject: String) {
      mainHandler.post {
        val act = instance ?: return@post
        val send = Intent(Intent.ACTION_SEND).apply {
          type = "text/plain"
          putExtra(Intent.EXTRA_TEXT, text)
          putExtra(Intent.EXTRA_SUBJECT, subject)
        }
        try {
          act.startActivity(Intent.createChooser(send, "分享备份"))
        } catch (_: Exception) { }
      }
    }

    /** 安装自动更新下载的 APK：FileProvider 授权 → ACTION_VIEW → 系统安装器（8.0+ 需 REQUEST_INSTALL_PACKAGES） */
    @JvmStatic
    fun installApk(path: String): String? {
      val result = java.util.concurrent.atomic.AtomicReference<String?>(null)
      mainHandler.post {
        val act = instance ?: run { result.set("app 未就绪"); return@post }
        try {
          val apk = File(path)
          if (!apk.exists()) { result.set("APK 文件不存在"); return@post }
          val uri = androidx.core.content.FileProvider.getUriForFile(act, act.packageName + ".fileprovider", apk)
          val intent = Intent(Intent.ACTION_VIEW).apply {
            setDataAndType(uri, "application/vnd.android.package-archive")
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
          }
          act.startActivity(intent)
          result.set("ok")
        } catch (e: Exception) {
          result.set("启动安装失败：" + e.message)
        }
      }
      try { Thread.sleep(600) } catch (_: InterruptedException) { }
      return result.get()
    }

    /** 打开外部浏览器访问指定 URL（蓝奏云下载页等） */
    @JvmStatic
    fun openUrl(url: String): String? {
      val result = java.util.concurrent.atomic.AtomicReference<String?>(null)
      mainHandler.post {
        val act = instance ?: run { result.set("app 未就绪"); return@post }
        try {
          val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url)).apply {
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
          }
          act.startActivity(intent)
          result.set("ok")
        } catch (e: Exception) {
          result.set("打开浏览器失败：" + e.message)
        }
      }
      try { Thread.sleep(600) } catch (_: InterruptedException) { }
      return result.get()
    }

    /** 跳转系统安全设置（SECURITY_SETTINGS 一定存在；BIOMETRIC_ENROLL 在部分 ROM 上 startActivity 成功但不显示，故不优先） */
    @JvmStatic
    fun openBiometricSettings(): String? {
      val opened = java.util.concurrent.atomic.AtomicReference<String?>(null)
      mainHandler.post {
        val act = instance ?: return@post
        val pm = act.packageManager
        val candidates = listOf(
          Settings.ACTION_SECURITY_SETTINGS,
          Settings.ACTION_BIOMETRIC_ENROLL
        )
        for (action in candidates) {
          try {
            val intent = Intent(action)
            if (intent.resolveActivity(pm) == null) continue
            act.startActivity(intent)
            opened.set(action)
            return@post
          } catch (_: Exception) { }
        }
      }
      // 给主线程一点时间完成跳转，再返回结果
      try { Thread.sleep(400) } catch (_: InterruptedException) { }
      return opened.get()
    }

    // ---------- 指纹会话密钥持久化（Android Keystore AES-GCM 加密） ----------
    // 目的：App 退出/冷启动后仍能用指纹解锁（bio_key 落盘但被 Keystore 硬件密钥保护）

    @JvmStatic
    fun saveBioKey(b64: String): Boolean {
      val act = instance ?: return false
      return try {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, getOrCreateBioKey(act))
        val enc = cipher.doFinal(b64.toByteArray(Charsets.UTF_8))
        val out = ByteArray(12 + enc.size)
        System.arraycopy(cipher.iv, 0, out, 0, 12)
        System.arraycopy(enc, 0, out, 12, enc.size)
        File(act.filesDir, "bio_key.enc").writeBytes(Base64.encode(out, Base64.NO_WRAP))
        true
      } catch (_: Exception) { false }
    }

    @JvmStatic
    fun loadBioKey(): String? {
      val act = instance ?: return null
      return try {
        val f = File(act.filesDir, "bio_key.enc")
        if (!f.exists()) return null
        val raw = Base64.decode(f.readBytes(), Base64.NO_WRAP)
        if (raw.size < 12 + 16) return null
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, getOrCreateBioKey(act), GCMParameterSpec(128, raw, 0, 12))
        String(cipher.doFinal(raw, 12, raw.size - 12), Charsets.UTF_8)
      } catch (_: Exception) { null }
    }

    @JvmStatic
    fun deleteBioKey() {
      try { instance?.filesDir?.let { File(it, "bio_key.enc").delete() } } catch (_: Exception) { }
    }

    private fun getOrCreateBioKey(act: android.app.Activity): SecretKey {
      val ks = KeyStore.getInstance("AndroidKeyStore")
      ks.load(null)
      val existing = ks.getKey(BIO_KS_ALIAS, null)
      if (existing != null) return existing as SecretKey
      val kg = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
      kg.init(
        KeyGenParameterSpec.Builder(BIO_KS_ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
          .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
          .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
          .setKeySize(256)
          .build()
      )
      return kg.generateKey()
    }
  }
}
