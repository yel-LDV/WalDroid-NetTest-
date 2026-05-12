@file:Suppress("unused")

package mx.angeldl.walet.core

import android.content.Context
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey

object EncryptedSeedStore {
    private const val PREFS_NAME = "encrypted_seeds"

    private fun getPrefs(context: Context) = EncryptedSharedPreferences.create(
        context,
        PREFS_NAME,
        MasterKey.Builder(context)
            .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
            .build(),
        EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
        EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM
    )

    fun saveSeed(context: Context, alias: String, seed: String) {
        getPrefs(context).edit().putString("seed_$alias", seed).apply()
    }

    fun getSeed(context: Context, alias: String): String? {
        return getPrefs(context).getString("seed_$alias", null)
    }

    fun deleteSeed(context: Context, alias: String) {
        getPrefs(context).edit().remove("seed_$alias").apply()
    }
}
