# Faces

LightCraft reads the face names other apps (Lightroom, digiKam…) write into your photos, shows them in the loupe
(boxes you can switch off, resize and remove) and groups photos by person in the People view. This page is about the
*models* that find and recognise faces by themselves.

## Nothing is bundled; every model is an opt-in download

No model weights are part of LightCraft. Each one is a file you choose to get, from **Settings ▸ Faces**:

- **Face detection:** [YuNet](https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet) (232 KB, MIT).
  It runs in LightCraft's own code, with no extra runtime. It was trained on the WIDER FACE dataset; an MIT licence on
  weights does not settle that dataset's terms, so this is flagged for the maintainer. It finds faces; it does not
  identify anyone. **Photo ▸ Detect Faces** offers the download the first time you use it.
- **Face recognition models** are large (AuraFace is 261 MB), and their licences and training data deserve a decision by
  the person installing them. Everything works without one: names from XMP, the People view, the face boxes.

## Downloading a model

**Settings ▸ Faces ▸ Download** (or Photo ▸ Detect Faces, which offers it for the detector):

1. A dialog shows the model's licence, whether commercial use is allowed, what it was trained on (or that this is not
   known) and which site the file comes from. **Download stays disabled until you tick "I have read these terms and
   accept them for my own use".** Nothing is fetched before that.
2. LightCraft downloads the file from the model's own repository (GitHub for YuNet and SFace, Hugging Face for
   AuraFace), at an address pinned to a commit, over https, in pure Rust (the `lightcraft-fetch` crate: no `curl`, no
   OpenSSL). An interrupted download resumes where it stopped.
3. The file must match the model's recorded size and SHA-256, or it is thrown away. A file that matches is installed
   by itself. Nothing else is sent anywhere, and no token or account is involved.

**Open page** opens the model's own page in your browser if you would rather get the file yourself.

## Adding a model file yourself

1. **Settings ▸ Faces ▸ Add a model file…**, or drop a `.onnx` file on the window.
2. LightCraft looks at the file (it never runs it at this point): it recognises models it knows by their SHA-256, and
   for any other file it reads the input and output shapes and describes what it assumed (112 × 112 aligned faces,
   RGB, `(x − 127.5) / 127.5`, one vector per face: the ArcFace convention that InsightFace models also use).
3. The same dialog shows the licence, whether commercial use is allowed and what the model was trained on, and
   **Install stays disabled until you tick the acceptance box**.
4. The model is copied into LightCraft's models folder and checked against the original by hash.

Non-commercial models (InsightFace, for example) can be added this way for your own use; LightCraft never bundles,
hosts, downloads or links them from a picker, and the dialog says so.

The models folder is `<config>/models` (`%APPDATA%\LightCraft\models` on Windows, `~/Library/Application Support/LightCraft/models`
on macOS, `~/.config/lightcraft/models` on Linux), or `$LIGHTCRAFT_FACE_MODELS`. The desktop app, the CLI and the MCP
server share it.

## What is known about the models LightCraft recognises

| Model | Licence of the weights | Trained on | Notes |
| --- | --- | --- | --- |
| YuNet 2023mar (detector) | MIT | WIDER FACE | 232 KB |
| AuraFace v1 | Apache-2.0 | "a commercial dataset", undisclosed | 261 MB; the best measured on sculpted busts |
| SFace 2021dec | labelled Apache-2.0 | undocumented (the upstream repository mentions CASIA-WebFace, VGGFace2, MS1MV2) | 39 MB; two questions about commercial use are unanswered upstream |

"Commercial use allowed" in the dialog is the weights' licence; it says nothing about the training data.

## Finding faces yourself

**Photo ▸ Detect Faces** (`faces.detect`) runs the YuNet detector on the selected photos and adds what it finds as
unnamed face boxes, in one undo step. A new run replaces earlier detections; boxes that came from XMP, or that you drew or
named, are never touched (and a face that already has one is not boxed a second time), and no sidecar is written. It looks at the photo upright and uncropped with default settings,
so your edits and crops do not matter. `apply: false` only reports. It finds faces of about 10 pixels and up in a
640-pixel version of the photo (so very small faces in a large group photo can be missed; looking at tiles is planned).
The detector's output matches OpenCV's own YuNet on a 45-photo public-domain test set (97 of 99 faces found by both,
mean box overlap 0.97), including marble busts and paintings, with no false boxes on the landscape and architecture
photos in the set.

## Commands

All of this is reachable from the control channel, the CLI and MCP: `faces.models.list`, `faces.models.inspect {path}`,
`faces.models.install {path, acknowledged: true}`, `faces.models.download {id, acknowledged: true}` (then `faces.models.downloads` for
progress; the download is installed when it arrives, and `faces.models.downloadCancel {id}` stops it), `faces.models.remove {id}`, `faces.models.select {id}`,
`faces.enable {enabled?}`, and `faces.detect {ids?, apply?}`. `acknowledged` must be `true`: the caller has shown the user the terms and the user agreed.
