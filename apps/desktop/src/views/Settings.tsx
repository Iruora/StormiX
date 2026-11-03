import type { ReactNode } from "react";

import { MOD } from "./Menu";
import { ProxySetting } from "./ProxySetting";

/** Settings that apply to the whole app rather than to one test file. */
export function SettingsView({ version }: { version: string | null }) {
  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-wrap items-center gap-x-3 gap-y-2 border-b border-line bg-surface px-5 py-3">
        <h1 className="text-[0.9375rem] font-semibold">Settings</h1>
        <span className="min-w-0 flex-1 basis-48 text-xs text-muted">
          These apply to every workspace and are kept when Stormix restarts.
        </span>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto p-5">
        <div className="mx-auto max-w-3xl space-y-4">
          <Section title="Proxy for requests">
            <ProxySetting />
          </Section>
          <Section title="Keyboard">
            <dl className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-1.5 text-xs">
              {[
                [`${MOD} K`, "Search every action, environment and step"],
                [`${MOD} O`, "Open a test file"],
                [`${MOD} S`, "Save"],
                [`${MOD} ⇧ S`, "Save as"],
              ].map(([keys, what]) => (
                <div key={keys} className="contents">
                  <dt>
                    <kbd className="rounded border border-line bg-surface-2 px-1.5 py-0.5 font-sans text-[0.6875rem]">{keys}</kbd>
                  </dt>
                  <dd className="text-muted">{what}</dd>
                </div>
              ))}
            </dl>
          </Section>
          <Section title="About">
            <p className="text-xs text-muted">
              Stormix {version ? `v${version}` : ""}. New releases are offered at launch, above the editor.
            </p>
          </Section>
        </div>
      </div>
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="rounded-lg border border-line bg-surface">
      <h2 className="border-b border-line px-4 py-2.5 text-[0.8125rem] font-semibold">{title}</h2>
      <div className="px-4 py-3">{children}</div>
    </section>
  );
}
