import { useEffect, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { Badge, ErrorNote, Field, formatDateTime, messageOf, runAction, useCommand, type Stored } from "./ui";

const MAX_BYTES = 512;

function ModemSection() {
  const [text, setText] = useState("");
  const [airTime, setAirTime] = useState<number | null>(null);
  const [busy, setBusy] = useState<"send" | "listen" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [received, setReceived] = useState<string | null>(null);
  const [listenSeconds, setListenSeconds] = useState("30");

  const bytes = new TextEncoder().encode(text.trim()).length;

  useEffect(() => {
    if (!text.trim()) {
      setAirTime(null);
      return;
    }
    let alive = true;
    void invoke<number>("modem_air_time", { text }).then((s) => alive && setAirTime(s)).catch(() => {});
    return () => {
      alive = false;
    };
  }, [text]);

  const guard = async (kind: "send" | "listen", work: () => Promise<void>) => {
    setBusy(kind);
    setError(null);
    setStatus(null);
    try {
      await work();
    } catch (e) {
      setError(messageOf(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <section className="glass-panel fo-panel" aria-label="Data over a voice radio">
      <h2>Data over a voice radio</h2>
      <p className="fo-lead">
        Send text through any radio that carries a voice: a handheld, CB, marine or HF set. Wire the radio's microphone
        and speaker jacks to this computer, or hold them to its speaker and microphone. The other station needs ATHENA
        too. Messages are checked on arrival: one that was damaged on the way is rejected, never shown garbled.
      </p>
      <ErrorNote message={error} />
      {status ? <p className="fo-small">{status}</p> : null}

      <h3>Send</h3>
      <Field label={`Message (${bytes} of ${MAX_BYTES} bytes${airTime != null ? `, about ${Math.ceil(airTime)} seconds on air` : ""})`}>
        <textarea value={text} onChange={(e) => setText(e.target.value)} placeholder="Ridge to base: bridge out at the ford, use the north track." />
      </Field>
      <div className="fo-row">
        <button
          type="button"
          className="secondary"
          disabled={busy != null || !text.trim() || bytes > MAX_BYTES}
          onClick={() =>
            void guard("send", async () => {
              const seconds = await invoke<number>("modem_transmit", { text });
              setStatus(`Sent (${seconds.toFixed(1)} seconds of audio).`);
            })
          }
        >
          {busy === "send" ? "Playing…" : "Key the radio, then play now"}
        </button>
        <button
          type="button"
          className="secondary"
          disabled={busy != null || !text.trim() || bytes > MAX_BYTES}
          onClick={() =>
            void guard("send", async () => {
              const path = await save({ defaultPath: "athena-message.wav", filters: [{ name: "WAV audio", extensions: ["wav"] }] });
              if (!path) return;
              const info = await invoke<{ path: string; seconds: number }>("modem_write_audio", { text, path });
              setStatus(`Saved ${info.seconds.toFixed(1)} seconds of audio to ${info.path}. Play it into any radio.`);
            })
          }
        >
          Save as audio file…
        </button>
      </div>
      <p className="fo-small fo-muted">
        Press and hold the radio's transmit button before you press play, and release it when the tones stop. Set the
        volume so the tones are clear but not distorted. Transmitting on amateur or licensed bands needs the right
        licence in normal times.
      </p>

      <h3>Receive</h3>
      <div className="fo-row">
        <Field label="Listen for (seconds)">
          <input className="fo-input fo-input--narrow" type="number" min={2} max={120} value={listenSeconds} onChange={(e) => setListenSeconds(e.target.value)} />
        </Field>
        <button
          type="button"
          className="secondary"
          disabled={busy != null}
          onClick={() =>
            void guard("listen", async () => {
              setReceived(null);
              setReceived(await invoke<string>("modem_listen", { seconds: Number.parseInt(listenSeconds, 10) || 30 }));
            })
          }
        >
          {busy === "listen" ? "Listening…" : "Listen now"}
        </button>
        <button
          type="button"
          className="secondary"
          disabled={busy != null}
          onClick={() =>
            void guard("listen", async () => {
              const path = await open({ multiple: false, filters: [{ name: "WAV audio", extensions: ["wav"] }] });
              if (typeof path !== "string") return;
              setReceived(null);
              setReceived(await invoke<string>("modem_read_audio", { path }));
            })
          }
        >
          Read from a recording…
        </button>
      </div>
      {received != null ? (
        <article className="glass-panel fo-card" aria-live="polite">
          <div className="fo-card-head">
            <strong>Message received</strong>
            <Badge tone="good">Checksum good</Badge>
          </div>
          <p className="fo-prewrap">{received}</p>
        </article>
      ) : null}
    </section>
  );
}

type Intercept = Stored<{
  at: number;
  frequency: string;
  source: string;
  text: string;
  matched: string[];
  loggedBy: string | null;
}>;

type IntelView = {
  intercepts: Intercept[];
  watchWords: string[];
  digest: { total: number; matched: number; words: Array<[string, number]>; frequencies: Array<[string, number]> };
};

function InterceptsSection() {
  const { data, error, loading, reload } = useCommand<IntelView>("get_intel");
  const [actionError, setActionError] = useState<string | null>(null);
  const [frequency, setFrequency] = useState("");
  const [source, setSource] = useState("");
  const [text, setText] = useState("");
  const [words, setWords] = useState<string | null>(null);
  const [whisperUrl, setWhisperUrl] = useState("http://127.0.0.1:9001");
  const [transcribing, setTranscribing] = useState(false);
  const [onlyMatched, setOnlyMatched] = useState(false);

  const intercepts = (data?.intercepts ?? []).filter((i) => !onlyMatched || i.matched.length > 0);
  const d = data?.digest;

  const transcribe = async () => {
    try {
      const path = await open({ multiple: false, filters: [{ name: "WAV audio", extensions: ["wav"] }] });
      if (typeof path !== "string") return;
      setTranscribing(true);
      const out = await invoke<string>("transcribe_intercept_recording", { path, whisperBaseUrl: whisperUrl });
      setText((t) => (t.trim() ? `${t.trim()}\n${out}` : out));
      setActionError(null);
    } catch (e) {
      setActionError(messageOf(e));
    } finally {
      setTranscribing(false);
    }
  };

  return (
    <section className="glass-panel fo-panel" aria-label="Radio intercepts">
      <h2>Radio intercepts</h2>
      <p className="fo-lead">
        Log what you hear on the radio: broadcasts, weather, other groups' traffic. Each entry is checked against your
        watch words, and a match raises a notification and shows on the Dashboard. A recording can be turned into text
        by the speech-to-text service instead of typing it.
      </p>
      <ErrorNote message={error ?? actionError} />

      {d && d.total > 0 ? (
        <p className="fo-small" aria-live="polite">
          Last 24 hours: {d.total} logged, {d.matched} with watch words
          {d.words.length > 0 ? ` (${d.words.slice(0, 6).map(([w, n]) => (n > 1 ? `${w} ×${n}` : w)).join(", ")})` : ""}.
        </p>
      ) : null}

      <form
        className="fo-cards"
        onSubmit={(e) => {
          e.preventDefault();
          void runAction(
            () => invoke("add_intercept", { intercept: { at: Math.floor(Date.now() / 1000), frequency, source, text } }),
            setActionError,
            async () => {
              setText("");
              await reload();
            },
          );
        }}
      >
        <div className="fo-row">
          <Field label="Where on the dial">
            <input className="fo-input" value={frequency} onChange={(e) => setFrequency(e.target.value)} placeholder="162.550 MHz" />
          </Field>
          <Field label="Who was transmitting (if known)" grow>
            <input className="fo-input" value={source} onChange={(e) => setSource(e.target.value)} placeholder="Weather radio" />
          </Field>
        </div>
        <Field label="What was said">
          <textarea value={text} onChange={(e) => setText(e.target.value)} />
        </Field>
        <div className="fo-row">
          <button type="submit" className="secondary" disabled={!text.trim()}>
            Log intercept
          </button>
          <button type="button" className="secondary" disabled={transcribing} onClick={() => void transcribe()}>
            {transcribing ? "Transcribing…" : "Transcribe a recording…"}
          </button>
          <Field label="Speech-to-text service address">
            <input className="fo-input" value={whisperUrl} onChange={(e) => setWhisperUrl(e.target.value)} />
          </Field>
        </div>
      </form>

      <h3>Watch words</h3>
      {words == null ? (
        <div className="fo-row">
          <span className="fo-small">{(data?.watchWords ?? []).join(", ") || "None"}</span>
          <button type="button" className="secondary" onClick={() => setWords((data?.watchWords ?? []).join(", "))}>
            Edit
          </button>
        </div>
      ) : (
        <form
          className="fo-row"
          onSubmit={(e) => {
            e.preventDefault();
            void runAction(
              () => invoke("set_intel_watch_words", { words: words.split(",").map((w) => w.trim()).filter(Boolean) }),
              setActionError,
              async () => {
                setWords(null);
                await reload();
              },
            );
          }}
        >
          <Field label="Words and phrases, separated by commas. Add local place names." grow>
            <input className="fo-input" value={words} onChange={(e) => setWords(e.target.value)} />
          </Field>
          <button type="submit" className="secondary">
            Save
          </button>
          <button type="button" className="secondary" onClick={() => setWords(null)}>
            Cancel
          </button>
        </form>
      )}
      <p className="fo-small fo-muted">Watch words apply to entries logged from now on.</p>

      <div className="fo-row">
        <h3>Log</h3>
        <label className="fo-small" style={{ display: "flex", gap: "0.35rem", alignItems: "center" }}>
          <input type="checkbox" checked={onlyMatched} onChange={(e) => setOnlyMatched(e.target.checked)} />
          Only entries with watch words
        </label>
      </div>
      {intercepts.length === 0 ? (
        <p className="fo-empty">{loading ? "Loading…" : "Nothing logged."}</p>
      ) : (
        <div className="fo-cards">
          {intercepts.slice(0, 100).map((i) => (
            <article key={i.id} className="glass-panel fo-card">
              <div className="fo-card-head">
                <span className="fo-small">
                  <strong>{formatDateTime(i.at)}</strong>
                  {i.frequency ? ` · ${i.frequency}` : ""}
                  {i.source ? ` · ${i.source}` : ""}
                </span>
                <span className="fo-actions">
                  {i.matched.map((w) => (
                    <Badge key={w} tone="warn">
                      {w}
                    </Badge>
                  ))}
                </span>
              </div>
              <p className="fo-small fo-prewrap">{i.text}</p>
              <div className="fo-card-head">
                <span className="fo-small fo-muted">{i.loggedBy ? `Logged by ${i.loggedBy}` : ""}</span>
                <button
                  type="button"
                  className="secondary"
                  onClick={() => void runAction(() => invoke("delete_intercept", { id: i.id }), setActionError, reload)}
                >
                  Remove
                </button>
              </div>
            </article>
          ))}
        </div>
      )}
    </section>
  );
}

export function RadioDataTab() {
  return (
    <div className="fo-cards">
      <ModemSection />
      <InterceptsSection />
    </div>
  );
}
