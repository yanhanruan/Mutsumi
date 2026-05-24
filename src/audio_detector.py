"""
System audio detection via Windows WASAPI peak meter (pycaw).
Polls the default output device every poll_ms milliseconds and emits
Qt signals when audio playback starts or stops.
"""
from __future__ import annotations

from PySide6.QtCore import QObject, QTimer, Signal

_THRESHOLD = 0.001  # peak values below this are treated as silence


def _is_audio_playing() -> bool:
    try:
        from pycaw.pycaw import AudioUtilities, IAudioMeterInformation

        for session in AudioUtilities.GetAllSessions():
            try:
                meter = session._ctl.QueryInterface(IAudioMeterInformation)
                if meter.GetPeakValue() > _THRESHOLD:
                    return True
            except Exception:
                continue
        return False
    except Exception as e:
        print(f'[AudioDetector] _is_audio_playing error: {e}')
        return False


class AudioDetector(QObject):
    audio_started = Signal()
    audio_stopped = Signal()

    def __init__(self, poll_ms: int = 500, parent=None):
        super().__init__(parent)
        self._playing = False
        self._poll_count = 0
        self._timer = QTimer(self)
        self._timer.timeout.connect(self._poll)
        self._timer.start(poll_ms)
        print('[AudioDetector] started, polling every', poll_ms, 'ms')

    def _poll(self):
        playing = _is_audio_playing()
        self._poll_count += 1
        # Print a status line every 10 polls (~5 s at 500 ms) so you can see it's alive
        if self._poll_count % 10 == 0:
            print(f'[AudioDetector] poll #{self._poll_count}: playing={playing}  (state was {self._playing})')

        if playing == self._playing:
            return
        self._playing = playing
        if playing:
            print('[AudioDetector] *** audio STARTED — emitting signal ***')
            self.audio_started.emit()
        else:
            print('[AudioDetector] *** audio STOPPED — emitting signal ***')
            self.audio_stopped.emit()
