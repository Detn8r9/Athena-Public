Place ONNX sentence-embedding assets here for production:

- model.onnx — e.g. sentence-transformers/all-MiniLM-L6-v2 exported to ONNX (384-d output or last_hidden_state + mean pooling).
- tokenizer.json — tokenizer from the same model directory.

Tauri bundles this folder as Resource/embedding/. Dev builds also read CARGO_MANIFEST_DIR/resources/embedding/.
