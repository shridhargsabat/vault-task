import { useMemo, useRef } from "react";
import { marked } from "marked";
import DOMPurify from "dompurify";
import {
  Bold,
  CheckSquare,
  Code,
  Heading3,
  Italic,
  Link2,
} from "lucide-react";
import { useDebouncedField } from "../../hooks/useDebouncedField";

/**
 * Markdown note-taker with toolbar, write/preview toggle and 300ms
 * debounced auto-save (flushed on task switch / unmount).
 */
export function NoteEditor({
  taskId,
  initial,
  onSave,
}: {
  taskId: string;
  initial: string;
  onSave: (taskId: string, markdown: string) => void;
}) {
  const [value, update, dirty] = useDebouncedField(
    taskId,
    initial,
    300,
    onSave,
  );
  const areaRef = useRef<HTMLTextAreaElement>(null);

  const previewHtml = useMemo(() => {
    if (!value.trim()) return "";
    const html = marked.parse(value, { breaks: true }) as string;
    return DOMPurify.sanitize(html);
  }, [value]);

  const [tab, setTab] = useTabReset(taskId);

  const wrap = (before: string, after: string = before) => {
    const el = areaRef.current;
    if (!el) {
      update(`${value}${before}${after}`);
      return;
    }
    const { selectionStart: s, selectionEnd: e } = el;
    update(value.slice(0, s) + before + value.slice(s, e) + after + value.slice(e));
    requestAnimationFrame(() => {
      el.focus();
      el.setSelectionRange(s + before.length, e + before.length);
    });
  };

  const prefixLine = (prefix: string) => {
    const el = areaRef.current;
    if (!el) {
      update(`${prefix}${value}`);
      return;
    }
    const { selectionStart: s } = el;
    const lineStart = value.lastIndexOf("\n", s - 1) + 1;
    const next = value.slice(0, lineStart) + prefix + value.slice(lineStart);
    update(next);
    requestAnimationFrame(() => {
      el.focus();
      el.setSelectionRange(s + prefix.length, s + prefix.length);
    });
  };

  const tools = [
    { title: "Bold", icon: <Bold className="h-3.5 w-3.5" />, run: () => wrap("**") },
    { title: "Italic", icon: <Italic className="h-3.5 w-3.5" />, run: () => wrap("*") },
    {
      title: "Heading",
      icon: <Heading3 className="h-3.5 w-3.5" />,
      run: () => prefixLine("### "),
    },
    {
      title: "Code",
      icon: <Code className="h-3.5 w-3.5" />,
      run: () => wrap("`"),
    },
    {
      title: "Checklist",
      icon: <CheckSquare className="h-3.5 w-3.5" />,
      run: () => prefixLine("- [ ] "),
    },
    {
      title: "Link",
      icon: <Link2 className="h-3.5 w-3.5" />,
      run: () => wrap("[", "](url)"),
    },
  ];

  return (
    <section className="flex min-h-0 flex-1 flex-col">
      <div className="mb-1.5 flex items-center gap-1">
        <div className="flex rounded-md bg-neutral-900 p-0.5">
          {(["write", "preview"] as const).map((t) => (
            <button
              key={t}
              onClick={() => setTab(t)}
              className={`rounded px-2.5 py-1 text-[11px] font-medium capitalize ${
                tab === t
                  ? "bg-neutral-700 text-neutral-100"
                  : "text-neutral-500 hover:text-neutral-300"
              }`}
            >
              {t}
            </button>
          ))}
        </div>
        <span className="ml-auto text-[10px] text-neutral-600">
          {dirty ? "Saving…" : "Auto-saved"}
        </span>
      </div>

      {tab === "write" ? (
        <>
          <div className="mb-1.5 flex gap-0.5 rounded-md bg-neutral-900 p-1">
            {tools.map((t) => (
              <button
                key={t.title}
                title={t.title}
                onClick={t.run}
                className="grid h-7 w-7 place-items-center rounded text-neutral-400 hover:bg-neutral-800 hover:text-neutral-100"
              >
                {t.icon}
              </button>
            ))}
          </div>
          <textarea
            ref={areaRef}
            value={value}
            onChange={(e) => update(e.currentTarget.value)}
            placeholder="Write a note in Markdown…"
            spellCheck={false}
            className="vt-scroll min-h-0 flex-1 resize-none rounded-lg bg-neutral-900 p-3 font-mono text-xs leading-relaxed outline-none placeholder:text-neutral-600"
          />
        </>
      ) : (
        <div className="vt-scroll vt-md min-h-0 flex-1 overflow-y-auto rounded-lg bg-neutral-900 p-3">
          {previewHtml ? (
            /* Safe: previewHtml is DOMPurify-sanitized above before render. */
            <div dangerouslySetInnerHTML={{ __html: previewHtml }} />
          ) : (
            <p className="text-xs text-neutral-600">Nothing to preview.</p>
          )}
        </div>
      )}
    </section>
  );
}

// Preview tab resets to Write when switching tasks.
import { useEffect, useState } from "react";
function useTabReset(taskId: string) {
  const [tab, setTab] = useState<"write" | "preview">("write");
  useEffect(() => {
    setTab("write");
  }, [taskId]);
  return [tab, setTab] as const;
}
