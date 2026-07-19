use std::{
    path::{Component, Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use async_trait::async_trait;
use futures_util::StreamExt;
use lyrit_application::{
    ApplicationError, ArtifactStore, ByteRange, ByteStream, MediaFacts, MediaInspector,
    StoredObject,
};
use lyrit_domain::AssetKind;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    process::Command,
    time::timeout,
};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct LocalArtifactStore {
    root: PathBuf,
}

impl LocalArtifactStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn resolve(&self, storage_key: &str) -> Result<PathBuf, ApplicationError> {
        let key = Path::new(storage_key);
        if storage_key.is_empty()
            || key.is_absolute()
            || key
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(ApplicationError::Artifact(
                "artifact key is not a safe relative path".to_owned(),
            ));
        }
        Ok(self.root.join(key))
    }
}

#[async_trait]
impl ArtifactStore for LocalArtifactStore {
    async fn put(
        &self,
        storage_key: &str,
        mut body: ByteStream<'_>,
        max_bytes: i64,
    ) -> Result<StoredObject, ApplicationError> {
        let final_path = self.resolve(storage_key)?;
        let parent = final_path
            .parent()
            .ok_or_else(|| ApplicationError::Artifact("artifact key has no parent".to_owned()))?;
        fs::create_dir_all(parent).await.map_err(artifact_error)?;
        let temporary_path = parent.join(format!(".upload-{}.part", Uuid::new_v4()));
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary_path)
            .await
            .map_err(artifact_error)?;
        let mut bytes = 0_i64;
        let mut hasher = Sha256::new();

        while let Some(chunk) = body.next().await {
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(error) => {
                    let _ = fs::remove_file(&temporary_path).await;
                    return Err(error);
                }
            };
            bytes = bytes
                .checked_add(
                    i64::try_from(chunk.len()).map_err(|_| ApplicationError::PayloadTooLarge)?,
                )
                .ok_or(ApplicationError::PayloadTooLarge)?;
            if bytes > max_bytes {
                let _ = fs::remove_file(&temporary_path).await;
                return Err(ApplicationError::PayloadTooLarge);
            }
            if let Err(error) = file.write_all(&chunk).await {
                let _ = fs::remove_file(&temporary_path).await;
                return Err(artifact_error(error));
            }
            hasher.update(&chunk);
        }

        if bytes == 0 {
            let _ = fs::remove_file(&temporary_path).await;
            return Err(ApplicationError::Validation(
                "uploaded file must not be empty".to_owned(),
            ));
        }
        if let Err(error) = file.flush().await {
            drop(file);
            let _ = fs::remove_file(&temporary_path).await;
            return Err(artifact_error(error));
        }
        if let Err(error) = file.sync_all().await {
            drop(file);
            let _ = fs::remove_file(&temporary_path).await;
            return Err(artifact_error(error));
        }
        drop(file);
        if let Err(error) = fs::rename(&temporary_path, &final_path).await {
            let _ = fs::remove_file(&temporary_path).await;
            return Err(artifact_error(error));
        }

        Ok(StoredObject {
            storage_key: storage_key.to_owned(),
            bytes,
            sha256: format!("{:x}", hasher.finalize()),
        })
    }

    async fn delete(&self, storage_key: &str) -> Result<(), ApplicationError> {
        let path = self.resolve(storage_key)?;
        match fs::remove_file(path).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(artifact_error(error)),
        }
    }

    async fn open(
        &self,
        storage_key: &str,
        range: ByteRange,
    ) -> Result<ByteStream<'static>, ApplicationError> {
        let path = self.resolve(storage_key)?;
        let mut file = fs::File::open(path).await.map_err(artifact_error)?;
        file.seek(std::io::SeekFrom::Start(range.start))
            .await
            .map_err(artifact_error)?;
        let remaining = range
            .end_inclusive
            .checked_sub(range.start)
            .and_then(|length| length.checked_add(1))
            .ok_or_else(|| ApplicationError::Artifact("invalid artifact range".to_owned()))?;
        let stream = futures_util::stream::try_unfold(
            (file, remaining),
            |(mut file, remaining)| async move {
                if remaining == 0 {
                    return Ok(None);
                }
                let chunk_size = usize::try_from(remaining.min(64 * 1024))
                    .expect("bounded artifact chunk size should fit usize");
                let mut buffer = vec![0_u8; chunk_size];
                let read = file.read(&mut buffer).await.map_err(artifact_error)?;
                if read == 0 {
                    return Err(ApplicationError::Artifact(
                        "artifact ended before its persisted byte count".to_owned(),
                    ));
                }
                buffer.truncate(read);
                Ok(Some((
                    bytes::Bytes::from(buffer),
                    (file, remaining - read as u64),
                )))
            },
        );
        Ok(Box::pin(stream))
    }
}

#[derive(Debug, Clone)]
pub struct FfprobeMediaInspector {
    artifact_root: PathBuf,
    executable: String,
    timeout: Duration,
}

impl FfprobeMediaInspector {
    pub fn new(artifact_root: impl Into<PathBuf>, executable: impl Into<String>) -> Self {
        Self {
            artifact_root: artifact_root.into(),
            executable: executable.into(),
            timeout: Duration::from_secs(20),
        }
    }

    fn resolve(&self, storage_key: &str) -> Result<PathBuf, ApplicationError> {
        let key = Path::new(storage_key);
        if storage_key.is_empty()
            || key.is_absolute()
            || key
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(ApplicationError::MediaInspection(
                "media key is not a safe relative path".to_owned(),
            ));
        }
        Ok(self.artifact_root.join(key))
    }
}

#[async_trait]
impl MediaInspector for FfprobeMediaInspector {
    async fn inspect(
        &self,
        storage_key: &str,
        kind: AssetKind,
    ) -> Result<MediaFacts, ApplicationError> {
        let path = self.resolve(storage_key)?;
        let mut command = Command::new(&self.executable);
        command
            .arg("-v")
            .arg("error")
            .arg("-show_entries")
            .arg("format=format_name,duration:stream=codec_type,codec_name,width,height")
            .arg("-of")
            .arg("json")
            .arg(&path)
            .stdin(Stdio::null())
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true);
        let output = timeout(self.timeout, command.output())
            .await
            .map_err(|_| ApplicationError::MediaInspection("ffprobe timed out".to_owned()))?
            .map_err(|error| ApplicationError::MediaInspection(error.to_string()))?;
        if !output.status.success() {
            return Err(ApplicationError::UnsupportedMedia(
                "the uploaded file could not be decoded as supported media".to_owned(),
            ));
        }
        let probe: ProbeOutput = serde_json::from_slice(&output.stdout)
            .map_err(|error| ApplicationError::MediaInspection(error.to_string()))?;
        facts_from_probe(kind, probe)
    }
}

#[derive(Debug, Deserialize)]
struct ProbeOutput {
    #[serde(default)]
    streams: Vec<ProbeStream>,
    #[serde(default)]
    format: ProbeFormat,
}

#[derive(Debug, Deserialize)]
struct ProbeStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<i32>,
    height: Option<i32>,
}

#[derive(Debug, Default, Deserialize)]
struct ProbeFormat {
    format_name: Option<String>,
    duration: Option<String>,
}

fn facts_from_probe(kind: AssetKind, probe: ProbeOutput) -> Result<MediaFacts, ApplicationError> {
    let format_name = probe.format.format_name.unwrap_or_default();
    match kind {
        AssetKind::Audio => {
            let stream = probe
                .streams
                .iter()
                .find(|stream| stream.codec_type.as_deref() == Some("audio"))
                .ok_or_else(|| {
                    ApplicationError::UnsupportedMedia(
                        "the uploaded file does not contain an audio stream".to_owned(),
                    )
                })?;
            let codec = stream.codec_name.as_deref().unwrap_or("unknown");
            if !is_supported_audio_codec(codec) {
                return Err(ApplicationError::UnsupportedMedia(format!(
                    "audio codec {codec} is not supported"
                )));
            }
            let duration_ms = parse_duration_ms(probe.format.duration.as_deref())?;
            Ok(MediaFacts {
                media_type: audio_media_type(&format_name, codec).to_owned(),
                duration_ms: Some(duration_ms),
                width: None,
                height: None,
                tool_metadata: json!({
                    "ffprobe": { "format_name": format_name, "codec_name": codec }
                }),
            })
        }
        AssetKind::Background => {
            let stream = probe
                .streams
                .iter()
                .find(|stream| stream.codec_type.as_deref() == Some("video"))
                .ok_or_else(|| {
                    ApplicationError::UnsupportedMedia(
                        "the uploaded file does not contain an image stream".to_owned(),
                    )
                })?;
            let codec = stream.codec_name.as_deref().unwrap_or("unknown");
            let media_type = match codec {
                "png" => "image/png",
                "mjpeg" | "jpeg2000" => "image/jpeg",
                "webp" => "image/webp",
                _ => {
                    return Err(ApplicationError::UnsupportedMedia(format!(
                        "background image codec {codec} is not supported"
                    )));
                }
            };
            let (width, height) = stream.width.zip(stream.height).ok_or_else(|| {
                ApplicationError::UnsupportedMedia(
                    "background image dimensions could not be determined".to_owned(),
                )
            })?;
            if width <= 0 || height <= 0 {
                return Err(ApplicationError::UnsupportedMedia(
                    "background image dimensions are invalid".to_owned(),
                ));
            }
            Ok(MediaFacts {
                media_type: media_type.to_owned(),
                duration_ms: None,
                width: Some(width),
                height: Some(height),
                tool_metadata: json!({
                    "ffprobe": { "format_name": format_name, "codec_name": codec }
                }),
            })
        }
        _ => Err(ApplicationError::Validation(
            "only source media can be inspected during upload".to_owned(),
        )),
    }
}

fn parse_duration_ms(duration: Option<&str>) -> Result<i64, ApplicationError> {
    let seconds = duration
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or_else(|| {
            ApplicationError::UnsupportedMedia("audio duration could not be determined".to_owned())
        })?;
    Ok((seconds * 1000.0).round() as i64)
}

fn is_supported_audio_codec(codec: &str) -> bool {
    matches!(codec, "aac" | "flac" | "mp3" | "opus" | "vorbis") || codec.starts_with("pcm_")
}

fn audio_media_type(format: &str, codec: &str) -> &'static str {
    if format.split(',').any(|name| name == "mp3") {
        "audio/mpeg"
    } else if format.split(',').any(|name| name == "wav") {
        "audio/wav"
    } else if format.split(',').any(|name| name == "flac") {
        "audio/flac"
    } else if format.split(',').any(|name| name == "ogg") {
        "audio/ogg"
    } else if format.split(',').any(|name| name == "webm") {
        "audio/webm"
    } else if codec == "aac" {
        "audio/mp4"
    } else {
        "application/octet-stream"
    }
}

fn artifact_error(error: std::io::Error) -> ApplicationError {
    ApplicationError::Artifact(error.to_string())
}

#[cfg(test)]
mod tests {
    use futures_util::TryStreamExt;

    use super::*;

    #[test]
    fn rejects_unsafe_artifact_keys() {
        let store = LocalArtifactStore::new("/tmp/lyrit-media-test");
        assert!(store.resolve("../escape").is_err());
        assert!(store.resolve("/absolute").is_err());
        assert!(
            store
                .resolve("projects/project/assets/asset/source")
                .is_ok()
        );
    }

    #[test]
    fn parses_supported_audio_probe_facts() {
        let facts = facts_from_probe(
            AssetKind::Audio,
            ProbeOutput {
                streams: vec![ProbeStream {
                    codec_type: Some("audio".to_owned()),
                    codec_name: Some("mp3".to_owned()),
                    width: None,
                    height: None,
                }],
                format: ProbeFormat {
                    format_name: Some("mp3".to_owned()),
                    duration: Some("12.345".to_owned()),
                },
            },
        )
        .unwrap();
        assert_eq!(facts.media_type, "audio/mpeg");
        assert_eq!(facts.duration_ms, Some(12_345));
    }

    #[test]
    fn rejects_non_image_background_streams() {
        let error = facts_from_probe(
            AssetKind::Background,
            ProbeOutput {
                streams: vec![ProbeStream {
                    codec_type: Some("video".to_owned()),
                    codec_name: Some("h264".to_owned()),
                    width: Some(1920),
                    height: Some(1080),
                }],
                format: ProbeFormat::default(),
            },
        );
        assert!(matches!(error, Err(ApplicationError::UnsupportedMedia(_))));
    }

    #[tokio::test]
    async fn local_store_promotes_complete_files_and_rejects_oversize_streams() {
        let root = std::env::temp_dir().join(format!("lyrit-media-{}", Uuid::new_v4()));
        let store = LocalArtifactStore::new(&root);
        let stored = store
            .put(
                "projects/test/assets/good/source",
                Box::pin(futures_util::stream::iter(vec![Ok("hello".into())])),
                5,
            )
            .await
            .unwrap();
        assert_eq!(stored.bytes, 5);
        assert_eq!(
            stored.sha256,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        assert_eq!(
            fs::read(root.join("projects/test/assets/good/source"))
                .await
                .unwrap(),
            b"hello"
        );
        let chunks: Vec<bytes::Bytes> = store
            .open(
                "projects/test/assets/good/source",
                ByteRange {
                    start: 1,
                    end_inclusive: 3,
                },
            )
            .await
            .unwrap()
            .try_collect()
            .await
            .unwrap();
        assert_eq!(chunks.concat(), b"ell");

        let oversize = store
            .put(
                "projects/test/assets/large/source",
                Box::pin(futures_util::stream::iter(vec![Ok("too large".into())])),
                3,
            )
            .await;
        assert!(matches!(oversize, Err(ApplicationError::PayloadTooLarge)));
        assert!(!root.join("projects/test/assets/large/source").exists());
        let mut entries = fs::read_dir(root.join("projects/test/assets/large"))
            .await
            .unwrap();
        assert!(entries.next_entry().await.unwrap().is_none());
        fs::remove_dir_all(root).await.unwrap();
    }
}
