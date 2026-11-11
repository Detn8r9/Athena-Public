Bundled Ollama model seed (optional)

Place a pre-populated Ollama models directory here so first boot and Outpost USB
export work fully offline:

  blobs/
  manifests/

Default instruct tag (see ATHENA_DEFAULT_MODEL.txt under AthenaData/app/models/ollama):
  llama3.2:1b

At runtime Athena copies this tree into AthenaData/app/models/ollama, then seeds
AthenaData/user/models/ollama (OLLAMA_MODELS) when the user store is empty.

Do not put large user-downloaded models here — those belong under AthenaData/user/.
