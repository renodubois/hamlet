import { createSignal, For, Match, Show, Switch } from "solid-js";
import { LogOutIcon } from "./icons";
import Modal from "./modal";

type SectionId = "profile" | "test";

interface Section {
  id: SectionId;
  label: string;
  tabId: string;
  panelId: string;
}

const SECTIONS: Section[] = [
  {
    id: "profile",
    label: "User Profile",
    tabId: "settings-tab-profile",
    panelId: "settings-panel-profile",
  },
  { id: "test", label: "Test Section", tabId: "settings-tab-test", panelId: "settings-panel-test" },
];

export default function SettingsModal(props: {
  open: boolean;
  onClose: () => void;
  onLogout: () => Promise<void>;
}) {
  const [section, setSection] = createSignal<SectionId>("profile");
  const [confirmLogout, setConfirmLogout] = createSignal(false);
  const [loggingOut, setLoggingOut] = createSignal(false);
  const active = () => SECTIONS.find((s) => s.id === section()) ?? SECTIONS[0];

  const handleConfirmLogout = async () => {
    setLoggingOut(true);
    try {
      await props.onLogout();
    } finally {
      setLoggingOut(false);
      setConfirmLogout(false);
    }
  };

  return (
    <>
      <Modal open={props.open} onClose={props.onClose} title="Settings" size="lg">
        <div class="flex gap-4 min-h-64">
          <div class="flex flex-col w-40 border-r border-gray-700 pr-2">
            <div
              role="tablist"
              aria-orientation="vertical"
              aria-label="Settings sections"
              class="flex flex-col"
            >
              <For each={SECTIONS}>
                {(s) => {
                  const selected = () => section() === s.id;
                  return (
                    <button
                      type="button"
                      role="tab"
                      id={s.tabId}
                      aria-selected={selected()}
                      aria-controls={s.panelId}
                      tabIndex={selected() ? 0 : -1}
                      class={`text-left px-3 py-2 rounded text-sm mb-1 ${
                        selected()
                          ? "bg-gray-700 text-white font-medium"
                          : "text-gray-300 hover:bg-gray-700 hover:text-gray-100"
                      }`}
                      onClick={() => setSection(s.id)}
                    >
                      {s.label}
                    </button>
                  );
                }}
              </For>
            </div>
            <div class="mt-auto pt-2 border-t border-gray-700">
              <button
                type="button"
                class="w-full flex items-center gap-2 text-left px-3 py-2 rounded text-sm text-red-400 hover:bg-gray-700 hover:text-red-300"
                onClick={() => setConfirmLogout(true)}
              >
                <LogOutIcon size={16} aria-hidden="true" />
                Log Out
              </button>
            </div>
          </div>
          <div
            role="tabpanel"
            id={active().panelId}
            aria-labelledby={active().tabId}
            class="flex-1 text-sm text-gray-200"
          >
            <Switch>
              <Match when={section() === "profile"}>
                <p>User profile settings go here.</p>
              </Match>
              <Match when={section() === "test"}>
                <p>Test section content.</p>
              </Match>
            </Switch>
          </div>
        </div>
      </Modal>

      <Modal
        open={confirmLogout()}
        onClose={() => !loggingOut() && setConfirmLogout(false)}
        title="Log out?"
      >
        <p class="text-sm text-gray-200 mb-4">Are you sure you want to log out?</p>
        <Show when={loggingOut()}>
          <p class="text-sm text-gray-400 mb-2">Logging out...</p>
        </Show>
        <div class="flex gap-2 justify-end">
          <button
            type="button"
            class="text-gray-300 hover:text-gray-100 text-sm px-3 py-2 disabled:opacity-50"
            onClick={() => setConfirmLogout(false)}
            disabled={loggingOut()}
          >
            Cancel
          </button>
          <button
            type="button"
            class="bg-red-600 hover:bg-red-700 text-white rounded-md px-4 py-2 text-sm font-medium disabled:opacity-50 transition-colors"
            onClick={handleConfirmLogout}
            disabled={loggingOut()}
          >
            Log out
          </button>
        </div>
      </Modal>
    </>
  );
}
