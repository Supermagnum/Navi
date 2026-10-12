package no.navi.app

import android.media.AudioManager
import android.media.ToneGenerator

/** Host-owned alarm audio hook: the plugin reports a level, the host makes the sound. */
interface ImuAlarmTonePlayer {
    fun play(tone: ImuAlarmTone)

    fun release()
}

/** Simple alarm-stream beeps via [ToneGenerator]; failures never reach the caller. */
class ToneImuAlarmPlayer : ImuAlarmTonePlayer {
    private var generator: ToneGenerator? = null

    @Synchronized
    override fun play(tone: ImuAlarmTone) {
        val gen = generator ?: createGenerator() ?: return
        val (toneType, durationMs) =
            when (tone) {
                ImuAlarmTone.WARNING -> ToneGenerator.TONE_PROP_BEEP2 to WARNING_MS
                ImuAlarmTone.DANGER -> ToneGenerator.TONE_CDMA_ABBR_ALERT to DANGER_MS
            }
        runCatching { gen.startTone(toneType, durationMs) }
    }

    @Synchronized
    override fun release() {
        runCatching { generator?.release() }
        generator = null
    }

    private fun createGenerator(): ToneGenerator? =
        runCatching { ToneGenerator(AudioManager.STREAM_ALARM, VOLUME_PERCENT) }
            .getOrNull()
            ?.also { generator = it }

    private companion object {
        const val VOLUME_PERCENT = 90
        const val WARNING_MS = 200
        const val DANGER_MS = 500
    }
}
