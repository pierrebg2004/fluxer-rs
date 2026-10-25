// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! Entrées/sorties audio natives via ALSA, chargé dynamiquement.
//!
//! Le projet n'ajoute aucune dépendance de compilation C : `libasound.so.2`
//! est résolu à l'exécution avec `libloading`, exactement comme le chargeur
//! Vulkan. Les quelques symboles utilisés sont déclarés ici en FFI Rust.

use std::ffi::{CStr, c_char, c_int, c_void};
use std::ptr;
use std::sync::Arc;

use libloading::Library;

const SND_PCM_STREAM_PLAYBACK: c_int = 0;
const SND_PCM_STREAM_CAPTURE: c_int = 1;
const SND_PCM_ACCESS_RW_INTERLEAVED: c_int = 3;
const SND_PCM_FORMAT_S16_LE: c_int = 2;
const RATE: u32 = 48_000;
const CHANNELS: usize = 2;

type PcmOpen = unsafe extern "C" fn(*mut *mut c_void, *const c_char, c_int, c_int) -> c_int;
type PcmClose = unsafe extern "C" fn(*mut c_void) -> c_int;
type PcmHwParamsMalloc = unsafe extern "C" fn(*mut *mut c_void) -> c_int;
type PcmHwParamsFree = unsafe extern "C" fn(*mut c_void);
type PcmHwParamsAny = unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int;
type PcmHwParamsSetAccess = unsafe extern "C" fn(*mut c_void, *mut c_void, c_int) -> c_int;
type PcmHwParamsSetFormat = unsafe extern "C" fn(*mut c_void, *mut c_void, c_int) -> c_int;
type PcmHwParamsSetChannels = unsafe extern "C" fn(*mut c_void, *mut c_void, c_int) -> c_int;
type PcmHwParamsSetRateNear =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut u32, *mut c_int) -> c_int;
type PcmHwParams = unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int;
type PcmReadWrite = unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> isize;
type PcmRecover = unsafe extern "C" fn(*mut c_void, c_int, c_int) -> c_int;
type PcmPrepare = unsafe extern "C" fn(*mut c_void) -> c_int;
type PcmDrop = unsafe extern "C" fn(*mut c_void) -> c_int;
type StrError = unsafe extern "C" fn(c_int) -> *const c_char;

/// Bibliothèque ALSA résolue à l'exécution.
struct Alsa {
    _library: Library,
    open: PcmOpen,
    close: PcmClose,
    hw_params_malloc: PcmHwParamsMalloc,
    hw_params_free: PcmHwParamsFree,
    hw_params_any: PcmHwParamsAny,
    set_access: PcmHwParamsSetAccess,
    set_format: PcmHwParamsSetFormat,
    set_channels: PcmHwParamsSetChannels,
    set_rate_near: PcmHwParamsSetRateNear,
    hw_params: PcmHwParams,
    readi: PcmReadWrite,
    writei: PcmReadWrite,
    recover: PcmRecover,
    prepare: PcmPrepare,
    drop: PcmDrop,
    strerror: StrError,
}

impl Alsa {
    fn load() -> Result<Arc<Self>, String> {
        // SAFETY: dlopen d'une bibliothèque système ; les symboles sont
        // copiés en pointeurs de fonction avant que la bibliothèque ne soit
        // conservée dans l'Arc.
        unsafe {
            let library = Library::new("libasound.so.2")
                .map_err(|error| format!("libasound.so.2 introuvable : {error}"))?;
            macro_rules! symbol {
                ($name:literal) => {{
                    let symbol: libloading::Symbol<'_, _> = library
                        .get($name)
                        .map_err(|error| format!("symbole ALSA {:?} absent : {error}", $name))?;
                    *symbol
                }};
            }
            Ok(Arc::new(Self {
                open: symbol!(b"snd_pcm_open\0"),
                close: symbol!(b"snd_pcm_close\0"),
                hw_params_malloc: symbol!(b"snd_pcm_hw_params_malloc\0"),
                hw_params_free: symbol!(b"snd_pcm_hw_params_free\0"),
                hw_params_any: symbol!(b"snd_pcm_hw_params_any\0"),
                set_access: symbol!(b"snd_pcm_hw_params_set_access\0"),
                set_format: symbol!(b"snd_pcm_hw_params_set_format\0"),
                set_channels: symbol!(b"snd_pcm_hw_params_set_channels\0"),
                set_rate_near: symbol!(b"snd_pcm_hw_params_set_rate_near\0"),
                hw_params: symbol!(b"snd_pcm_hw_params\0"),
                readi: symbol!(b"snd_pcm_readi\0"),
                writei: symbol!(b"snd_pcm_writei\0"),
                recover: symbol!(b"snd_pcm_recover\0"),
                prepare: symbol!(b"snd_pcm_prepare\0"),
                drop: symbol!(b"snd_pcm_drop\0"),
                strerror: symbol!(b"snd_strerror\0"),
                _library: library,
            }))
        }
    }

    fn error(&self, code: c_int) -> String {
        // SAFETY: snd_strerror renvoie toujours un pointeur statique valide.
        unsafe {
            let pointer = (self.strerror)(code);
            if pointer.is_null() {
                return format!("erreur ALSA {code}");
            }
            CStr::from_ptr(pointer).to_string_lossy().into_owned()
        }
    }
}

/// Handle PCM (capture ou lecture). Le flux est ouvert en S16_LE stéréo et
/// échantillonné à 48 kHz, format natif d'Opus.
pub struct Pcm {
    alsa: Arc<Alsa>,
    handle: *mut c_void,
}

// SAFETY: un `snd_pcm_t` peut être utilisé depuis un thread unique ; les
// threads audio sont créés et détruits avec le handle.
unsafe impl Send for Pcm {}

impl Drop for Pcm {
    fn drop(&mut self) {
        // SAFETY: handle ouvert par snd_pcm_open et non fermé ailleurs.
        unsafe {
            let _ = (self.alsa.drop)(self.handle);
            let _ = (self.alsa.close)(self.handle);
        }
    }
}

impl Pcm {
    /// Ouvre un flux en essayant plusieurs noms de périphérique : `default`
    /// passe par le serveur audio de la session (PipeWire/PulseAudio).
    fn open(alsa: Arc<Alsa>, stream: c_int) -> Result<Self, String> {
        let mut last = String::new();
        for name in ["default", "pipewire", "pulse", "sysdefault"] {
            let c_name = std::ffi::CString::new(name).expect("nom ALSA sans NUL");
            let mut handle: *mut c_void = ptr::null_mut();
            // SAFETY: arguments conformes à l'API ALSA.
            let result = unsafe { (alsa.open)(&mut handle, c_name.as_ptr(), stream, 0) };
            if result < 0 {
                last = alsa.error(result);
                continue;
            }
            match Self::configure(alsa.clone(), handle) {
                Ok(()) => return Ok(Self { alsa, handle }),
                Err(error) => {
                    // SAFETY: handle valide ouvert ci-dessus.
                    unsafe {
                        let _ = (alsa.close)(handle);
                    }
                    last = error;
                }
            }
        }
        Err(format!(
            "aucun périphérique audio utilisable ({}) : {last}",
            if stream == SND_PCM_STREAM_CAPTURE {
                "capture"
            } else {
                "lecture"
            }
        ))
    }

    fn configure(alsa: Arc<Alsa>, handle: *mut c_void) -> Result<(), String> {
        let mut params: *mut c_void = ptr::null_mut();
        // SAFETY: suite d'appels ALSA standard ; chaque étape vérifie son code.
        unsafe {
            let code = (alsa.hw_params_malloc)(&mut params);
            if code < 0 {
                return Err(alsa.error(code));
            }
            let result = (|| {
                let code = (alsa.hw_params_any)(handle, params);
                if code < 0 {
                    return Err(alsa.error(code));
                }
                let code = (alsa.set_access)(handle, params, SND_PCM_ACCESS_RW_INTERLEAVED);
                if code < 0 {
                    return Err(alsa.error(code));
                }
                let code = (alsa.set_format)(handle, params, SND_PCM_FORMAT_S16_LE);
                if code < 0 {
                    return Err(alsa.error(code));
                }
                let code = (alsa.set_channels)(handle, params, CHANNELS as c_int);
                if code < 0 {
                    return Err(alsa.error(code));
                }
                let mut rate = RATE;
                let code = (alsa.set_rate_near)(handle, params, &mut rate, ptr::null_mut());
                if code < 0 {
                    return Err(alsa.error(code));
                }
                if rate != RATE {
                    return Err(format!(
                        "fréquence {rate} Hz non prise en charge (48000 Hz requis)"
                    ));
                }
                let code = (alsa.hw_params)(handle, params);
                if code < 0 {
                    return Err(alsa.error(code));
                }
                let code = (alsa.prepare)(handle);
                if code < 0 {
                    return Err(alsa.error(code));
                }
                Ok(())
            })();
            (alsa.hw_params_free)(params);
            result
        }
    }

    /// Lit des échantillons entrelacés. Renvoie le nombre de trames lues.
    pub fn read_i16(&self, buffer: &mut [i16]) -> Result<usize, String> {
        self.transfer(buffer, true)
    }

    /// Écrit des échantillons entrelacés. Renvoie le nombre de trames écrites.
    pub fn write_i16(&self, buffer: &[i16]) -> Result<usize, String> {
        self.transfer(buffer, false)
    }

    fn transfer(&self, buffer: &[i16], read: bool) -> Result<usize, String> {
        let frames = buffer.len() / CHANNELS;
        let pointer = buffer.as_ptr() as *mut c_void;
        for attempt in 0..3 {
            // SAFETY: buffer contient au moins `frames * CHANNELS` i16.
            let result = unsafe {
                if read {
                    (self.alsa.readi)(self.handle, pointer, frames)
                } else {
                    (self.alsa.writei)(self.handle, pointer, frames)
                }
            };
            if result >= 0 {
                return Ok(result as usize);
            }
            let code = result as c_int;
            // SAFETY: récupération standard après underrun/overrun (-EPIPE).
            unsafe {
                let recovered = (self.alsa.recover)(self.handle, code, 1);
                if recovered < 0 {
                    let _ = (self.alsa.prepare)(self.handle);
                }
            }
            if attempt == 2 {
                return Err(self.alsa.error(code));
            }
        }
        Err("transfert audio interrompu".into())
    }
}

/// Ouvre la capture et la lecture en même temps, si possible.
pub struct Devices {
    pub capture: Option<Pcm>,
    pub playback: Option<Pcm>,
}

impl Devices {
    pub fn open() -> Result<Self, String> {
        let alsa = Alsa::load()?;
        Ok(Self {
            capture: Pcm::open(alsa.clone(), SND_PCM_STREAM_CAPTURE).ok(),
            playback: Pcm::open(alsa, SND_PCM_STREAM_PLAYBACK).ok(),
        })
    }
}

/// Taille d'une trame Opus de 20 ms à 48 kHz, en échantillons par canal.
pub const FRAME_SAMPLES: usize = 960;
