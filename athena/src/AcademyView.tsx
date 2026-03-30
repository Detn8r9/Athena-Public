import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ArrowLeft, CheckCircle2 } from "lucide-react";

interface AcademyLesson {
  id: string;
  title: string;
  zimNameHint: string;
  articleUrl: string;
}

interface AcademySubject {
  id: string;
  label: string;
  lessons: AcademyLesson[];
}

interface AcademyAgeGroup {
  id: string;
  label: string;
  subjects: AcademySubject[];
}

interface AcademyCurriculumRoot {
  ageGroups: AcademyAgeGroup[];
}

interface AcademyProgressRow {
  lessonId: string;
  completedAt: number;
}

interface AcademyLessonHtml {
  title: string;
  html: string;
}

type Nav =
  | { level: "ages" }
  | { level: "subjects"; age: AcademyAgeGroup }
  | { level: "lessons"; age: AcademyAgeGroup; subject: AcademySubject };

export function AcademyView() {
  const [curriculum, setCurriculum] = useState<AcademyCurriculumRoot | null>(null);
  const [completedIds, setCompletedIds] = useState<Set<string>>(new Set());
  const [nav, setNav] = useState<Nav>({ level: "ages" });
  const [activeLesson, setActiveLesson] = useState<AcademyLesson | null>(null);
  const [lessonHtml, setLessonHtml] = useState<string | null>(null);
  const [lessonTitle, setLessonTitle] = useState<string>("");
  const [loadErr, setLoadErr] = useState<string | null>(null);
  const [loadingLesson, setLoadingLesson] = useState(false);

  const refreshProgress = useCallback(async () => {
    try {
      const rows = await invoke<AcademyProgressRow[]>("academy_list_completed");
      const next = new Set<string>();
      for (const r of rows) {
        if (r.lessonId) next.add(r.lessonId);
      }
      setCompletedIds(next);
    } catch {
      setCompletedIds(new Set());
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const c = await invoke<AcademyCurriculumRoot>("academy_get_curriculum");
        if (!cancelled) setCurriculum(c);
      } catch (e) {
        if (!cancelled) setLoadErr(e != null ? String(e) : "Could not load curriculum.");
      }
    })();
    void refreshProgress();
    return () => {
      cancelled = true;
    };
  }, [refreshProgress]);

  async function openLesson(lesson: AcademyLesson) {
    setActiveLesson(lesson);
    setLessonHtml(null);
    setLessonTitle(lesson.title);
    setLoadingLesson(true);
    setLoadErr(null);
    try {
      const res = await invoke<AcademyLessonHtml>("academy_get_lesson_html", { args: { lessonId: lesson.id } });
      setLessonTitle(res.title);
      setLessonHtml(res.html);
    } catch (e) {
      setLoadErr(e != null ? String(e) : "Could not load lesson.");
    } finally {
      setLoadingLesson(false);
    }
  }

  function closeLesson() {
    setActiveLesson(null);
    setLessonHtml(null);
    setLoadErr(null);
  }

  async function markComplete() {
    if (!activeLesson) return;
    try {
      await invoke("academy_mark_lesson_complete", { args: { lessonId: activeLesson.id } });
      await refreshProgress();
    } catch (e) {
      setLoadErr(e != null ? String(e) : "Could not save progress.");
    }
  }

  if (!curriculum) {
    return (
      <div className="glass-panel app-view-placeholder" style={{ padding: "1rem" }}>
        <h2 className="app-view-placeholder-title">Academy</h2>
        <p className="text-muted">{loadErr ?? "Loading curriculum…"}</p>
      </div>
    );
  }

  if (activeLesson) {
    return (
      <div className="glass-panel app-view-placeholder" style={{ display: "grid", gap: "0.75rem", padding: "1rem" }}>
        <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", alignItems: "center" }}>
          <button type="button" className="secondary" onClick={() => closeLesson()}>
            <ArrowLeft size={16} strokeWidth={2} aria-hidden style={{ verticalAlign: "middle", marginRight: "0.35rem" }} />
            Back
          </button>
          <span style={{ fontWeight: 600 }}>{lessonTitle}</span>
          {completedIds.has(activeLesson.id) ? (
            <span className="text-muted" style={{ display: "inline-flex", alignItems: "center", gap: "0.25rem" }}>
              <CheckCircle2 size={16} aria-hidden />
              Completed
            </span>
          ) : null}
        </div>
        {loadErr ? (
          <p className="error" role="alert">
            {loadErr}
          </p>
        ) : null}
        <div className="glass-panel" style={{ padding: 0, overflow: "hidden", minHeight: "50vh" }}>
          {loadingLesson ? (
            <p className="text-muted" style={{ padding: "1rem" }}>
              Loading article…
            </p>
          ) : lessonHtml ? (
            <iframe
              title={lessonTitle}
              sandbox=""
              srcDoc={lessonHtml}
              style={{ width: "100%", minHeight: "55vh", border: "none", display: "block", background: "var(--surface-solid, #fff)" }}
            />
          ) : (
            <p className="text-muted" style={{ padding: "1rem" }}>
              No content.
            </p>
          )}
        </div>
        <div>
          <button type="button" className="glass-action-btn" onClick={() => void markComplete()} disabled={!activeLesson || loadingLesson}>
            Mark lesson complete
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="glass-panel app-view-placeholder" style={{ display: "grid", gap: "0.75rem", padding: "1rem" }}>
      <h2 className="app-view-placeholder-title">Academy</h2>
      <p className="text-muted" style={{ margin: 0 }}>
        Offline lessons from your ZIM library. Complete a lesson to track progress locally.
      </p>

      {nav.level === "ages" ? (
        <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "grid", gap: "0.5rem" }}>
          {curriculum.ageGroups.map((age) => (
            <li key={age.id}>
              <button
                type="button"
                className="glass-action-btn"
                style={{ width: "100%", justifyContent: "flex-start" }}
                onClick={() => setNav({ level: "subjects", age })}
              >
                {age.label}
              </button>
            </li>
          ))}
        </ul>
      ) : null}

      {nav.level === "subjects" ? (
        <div className="glass-panel" style={{ padding: "0.75rem", display: "grid", gap: "0.5rem" }}>
          <button type="button" className="secondary" onClick={() => setNav({ level: "ages" })}>
            ← Age groups
          </button>
          <div style={{ fontWeight: 600 }}>{nav.age.label}</div>
          <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "grid", gap: "0.5rem" }}>
            {nav.age.subjects.map((sub) => (
              <li key={sub.id}>
                <button
                  type="button"
                  className="glass-action-btn"
                  style={{ width: "100%", justifyContent: "flex-start" }}
                  onClick={() => setNav({ level: "lessons", age: nav.age, subject: sub })}
                >
                  {sub.label}
                </button>
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      {nav.level === "lessons" ? (
        <div className="glass-panel" style={{ padding: "0.75rem", display: "grid", gap: "0.5rem" }}>
          <button type="button" className="secondary" onClick={() => setNav({ level: "subjects", age: nav.age })}>
            ← Subjects
          </button>
          <div style={{ fontWeight: 600 }}>
            {nav.age.label} · {nav.subject.label}
          </div>
          <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "grid", gap: "0.5rem" }}>
            {nav.subject.lessons.map((les) => (
              <li key={les.id}>
                <button
                  type="button"
                  className="glass-action-btn"
                  style={{ width: "100%", justifyContent: "space-between", display: "flex", alignItems: "center", gap: "0.5rem" }}
                  onClick={() => void openLesson(les)}
                >
                  <span>{les.title}</span>
                  {completedIds.has(les.id) ? <CheckCircle2 size={18} aria-label="Completed" /> : null}
                </button>
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </div>
  );
}
