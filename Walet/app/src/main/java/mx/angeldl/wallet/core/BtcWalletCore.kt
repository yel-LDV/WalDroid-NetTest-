@file:Suppress("unused", "FunctionName")

package mx.angelDL.wallet.core

/**
 * BtcWalletCore — Puente JNI para btc_wallet_core (Rust).
 * 
 * IMPORTANTE: El nombre de este paquete (mx.angelDL.wallet.core) DEBE coincidir 
 * exactamente con el definido en la librería nativa para que JNI funcione.
 */
object BtcWalletCore {

    init {
        try {
            System.loadLibrary("btc_wallet_core")
        } catch (e: UnsatisfiedLinkError) {
            throw RuntimeException(
                "No se pudo cargar libbtc_wallet_core.so. " +
                "Verifica que los archivos .so estén en app/src/main/jniLibs/{abi}/",
                e
            )
        }
    }

    // ── Métodos Nativos ────────────────────────────────────────

    external fun createWallet(): String
    external fun validateMnemonic(mnemonic: String): String
    external fun getAddress(mnemonic: String): String
    external fun getBalance(mnemonic: String): Long
    external fun sendBitcoin(mnemonic: String, toAddress: String, amountSat: Long, feeRate: Float): String
    external fun getBtcPrice(currency: String): Double
    external fun convertCurrency(amount: Double, from: String, to: String): Double

    // ── Helpers ────────────────────────────────────────────────

    fun balanceBtc(mnemonic: String): Double = try {
        getBalance(mnemonic) / 100_000_000.0
    } catch (e: Throwable) {
        0.0
    }
}
