package com.kamune_org.stegano

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.provider.OpenableColumns
import android.util.Log
import androidx.activity.result.ActivityResult
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class PickFilesArgs {
  var mimeTypes: ArrayList<String>? = null
  var multiple: Boolean? = null
}

@InvokeArg
class PickSaveArgs {
  var name: String? = null
  var mimeType: String? = null
}

@TauriPlugin
class PickerPlugin(private val activity: Activity) : Plugin(activity) {
  @Command
  fun pickFiles(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(PickFilesArgs::class.java)
      val types = args.mimeTypes?.filter { it.isNotBlank() }.orEmpty()
      val intent = Intent(Intent.ACTION_OPEN_DOCUMENT)
      intent.addCategory(Intent.CATEGORY_OPENABLE)
      intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
      intent.addFlags(Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION)
      if (types.size == 1) {
        intent.type = types[0]
      } else {
        intent.type = "*/*"
        if (types.size > 1) {
          intent.putExtra(Intent.EXTRA_MIME_TYPES, types.toTypedArray())
        }
      }
      intent.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, args.multiple ?: false)
      startActivityForResult(invoke, intent, "pickFilesResult")
    } catch (ex: Exception) {
      val message = ex.message ?: "Failed to pick files"
      Log.e("Stegano", message, ex)
      invoke.reject(message)
    }
  }

  @ActivityCallback
  fun pickFilesResult(invoke: Invoke, result: ActivityResult) {
    try {
      if (result.resultCode == Activity.RESULT_CANCELED) {
        val empty = JSObject()
        empty.put("files", JSArray())
        invoke.resolve(empty)
        return
      }
      if (result.resultCode != Activity.RESULT_OK) {
        invoke.reject("Failed to pick files")
        return
      }
      val data = result.data
      if (data == null) {
        invoke.reject("file picker returned no files")
        return
      }
      val uris = urisOf(data)
      if (uris.isEmpty()) {
        invoke.reject("file picker returned no files")
        return
      }
      val files = JSArray()
      for (uri in uris) {
        persist(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION)
        val file = JSObject()
        file.put("path", uri.toString())
        file.put("name", displayName(uri))
        file.put("size", displaySize(uri))
        files.put(file)
      }
      val body = JSObject()
      body.put("files", files)
      invoke.resolve(body)
    } catch (ex: Exception) {
      val message = ex.message ?: "Failed to read file pick result"
      Log.e("Stegano", message, ex)
      invoke.reject(message)
    }
  }

  @Command
  fun pickSavePath(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(PickSaveArgs::class.java)
      val intent = Intent(Intent.ACTION_CREATE_DOCUMENT)
      intent.addCategory(Intent.CATEGORY_OPENABLE)
      intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
      intent.addFlags(Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION)
      intent.type = args.mimeType?.takeIf { it.isNotBlank() } ?: "*/*"
      intent.putExtra(Intent.EXTRA_TITLE, args.name ?: "")
      startActivityForResult(invoke, intent, "pickSavePathResult")
    } catch (ex: Exception) {
      val message = ex.message ?: "Failed to pick save path"
      Log.e("Stegano", message, ex)
      invoke.reject(message)
    }
  }

  @ActivityCallback
  fun pickSavePathResult(invoke: Invoke, result: ActivityResult) {
    try {
      if (result.resultCode == Activity.RESULT_CANCELED) {
        invoke.resolve()
        return
      }
      if (result.resultCode != Activity.RESULT_OK) {
        invoke.reject("Failed to pick save path")
        return
      }
      val uri = result.data?.data
      if (uri == null) {
        invoke.reject("save picker returned no path")
        return
      }
      persist(
        uri,
        Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
      )
      invoke.resolveObject(uri.toString())
    } catch (ex: Exception) {
      val message = ex.message ?: "Failed to read save result"
      Log.e("Stegano", message, ex)
      invoke.reject(message)
    }
  }

  private fun urisOf(data: Intent): List<Uri> {
    val out = ArrayList<Uri>()
    val clip = data.clipData
    if (clip != null && clip.itemCount > 0) {
      for (i in 0 until clip.itemCount) {
        clip.getItemAt(i).uri?.let { out.add(it) }
      }
    } else {
      data.data?.let { out.add(it) }
    }
    return out
  }

  private fun persist(uri: Uri, flags: Int) {
    try {
      activity.contentResolver.takePersistableUriPermission(uri, flags)
    } catch (ex: Exception) {
      Log.w("Stegano", "persistable permission failed: ${ex.message}")
    }
  }

  private fun displayName(uri: Uri): String {
    val cursor = activity.contentResolver.query(
      uri,
      arrayOf(OpenableColumns.DISPLAY_NAME),
      null,
      null,
      null
    )
    cursor?.use {
      if (it.moveToFirst()) {
        val index = it.getColumnIndex(OpenableColumns.DISPLAY_NAME)
        if (index >= 0 && !it.isNull(index)) {
          val name = it.getString(index)
          if (!name.isNullOrEmpty()) {
            return name
          }
        }
      }
    }
    return uri.lastPathSegment?.takeIf { it.isNotEmpty() } ?: "file"
  }

  private fun displaySize(uri: Uri): Long {
    val cursor = activity.contentResolver.query(
      uri,
      arrayOf(OpenableColumns.SIZE),
      null,
      null,
      null
    ) ?: return 0
    cursor.use {
      if (!it.moveToFirst()) {
        return 0
      }
      val index = it.getColumnIndex(OpenableColumns.SIZE)
      if (index < 0 || it.isNull(index)) {
        return 0
      }
      val size = it.getLong(index)
      return if (size < 0) 0 else size
    }
  }
}
