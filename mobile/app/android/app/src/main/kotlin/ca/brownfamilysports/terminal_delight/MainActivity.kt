package ca.brownfamilysports.terminal_delight

import android.content.Context
import android.content.IntentFilter
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.os.Build
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import java.net.Inet4Address
import java.net.InetAddress

class MainActivity : FlutterActivity() {
    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        // What this phone can see of its own end of each route to the laptop,
        // so a route that fails can say why from here. A key is left out when
        // the phone would not say, which is not the same as "no".
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "td/here")
            .setMethodCallHandler { call, result ->
                if (call.method == "here") result.success(here()) else result.notImplemented()
            }
    }

    private fun here(): Map<String, Boolean> {
        val out = HashMap<String, Boolean>()
        // USB_STATE is a sticky system broadcast, so registering no receiver
        // hands back the latest one. `connected` means a computer is on the
        // other end of the cable, not merely power; `adb` is USB debugging.
        val filter = IntentFilter("android.hardware.usb.action.USB_STATE")
        val usb = try {
            if (Build.VERSION.SDK_INT >= 33) {
                registerReceiver(null, filter, Context.RECEIVER_NOT_EXPORTED)
            } else {
                registerReceiver(null, filter)
            }
        } catch (_: Exception) {
            null
        }
        usb?.extras?.let { x ->
            if (x.containsKey("connected")) out["cable"] = x.getBoolean("connected")
            if (x.containsKey("adb")) out["adb"] = x.getBoolean("adb")
        }
        val cm = getSystemService(ConnectivityManager::class.java) ?: return out
        try {
            var wifi = false
            var tailnet = false
            @Suppress("DEPRECATION")
            for (n in cm.allNetworks) {
                val caps = cm.getNetworkCapabilities(n) ?: continue
                if (caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI)) wifi = true
                if (caps.hasTransport(NetworkCapabilities.TRANSPORT_VPN)) {
                    val addrs = cm.getLinkProperties(n)?.linkAddresses ?: continue
                    if (addrs.any { inTailnet(it.address) }) tailnet = true
                }
            }
            out["wifi"] = wifi
            out["tailnet"] = tailnet
        } catch (_: Exception) {
            // Unknown, not off: leave both out.
        }
        return out
    }

    /** Tailscale gives every device an address in 100.64.0.0/10. */
    private fun inTailnet(a: InetAddress): Boolean {
        if (a !is Inet4Address) return false
        val b = a.address
        return (b[0].toInt() and 0xff) == 100 && (b[1].toInt() and 0xc0) == 64
    }
}
