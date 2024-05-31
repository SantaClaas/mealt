import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  JSX,
  Resource,
  Setter,
  createContext,
  createResource,
  onCleanup,
  useContext,
} from "solid-js";

const socket = new WebSocket("ws://localhost:3000/ws");
socket.addEventListener("message", async (event) => {
  if (typeof event.data === "string")
    throw new Error("Unexpected string as message event payload");

  await invoke("process_message", event.data);
});
const [groups, { mutate: setGroups }] = createResource(
  async () => (await invoke("get_groups")) as string[]
);
type AppState = {
  socket: WebSocket;
  groups: Resource<string[]>;
  setGroups: Setter<string[] | undefined>;
};
const state = { socket, groups, setGroups } satisfies AppState;
const AppContext = createContext(state);

export function SocketProvider(properties: { children: JSX.Element }) {
  return (
    <AppContext.Provider value={state}>
      {properties.children}
    </AppContext.Provider>
  );
}

export function useWebSocket(onmessage?: (event: MessageEvent) => any) {
  const { socket: webSocket } = useContext(AppContext);

  if (onmessage) {
    webSocket.addEventListener("message", onmessage);

    onCleanup(() => {
      webSocket.removeEventListener("message", onmessage);
    });
  }

  return (data: string | ArrayBufferLike | Blob | ArrayBufferView) =>
    webSocket.send(data);
}

export const useAppState = () => useContext(AppContext);

listen("join_group", (event) => {
  if (
    typeof event.payload !== "object" ||
    event.payload === null ||
    !("group_id" in event.payload) ||
    typeof event.payload.group_id !== "string"
  )
    throw new Error("Unexpected join group event payload");

  const group = event.payload.group_id;
  setGroups((groups) => (groups === undefined ? [group] : [...groups, group]));
});
