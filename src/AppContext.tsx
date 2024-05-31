import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  Accessor,
  JSX,
  Resource,
  Setter,
  createContext,
  createEffect,
  createResource,
  createSignal,
  onCleanup,
  useContext,
} from "solid-js";

const [groups, { mutate: setGroups }] = createResource(
  async () => (await invoke("get_groups")) as string[]
);

const [identity, { mutate: setIdentity }] = createResource(
  async () =>
    (await invoke("get_identity").catch((error) => {
      console.warn(
        "Could not get identity. But this might be expected if this is the first time the app is run",
        error
      );
      return undefined;
    })) as string
);

async function handleMessage(event: MessageEvent) {
  // Might remove redundant check later
  if (typeof event.data === "string")
    throw new Error("Unexpected string as message event payload");

  const data = event.data;
  if (!(data instanceof Blob))
    throw new Error("Unexpected non-blob as message event payload");

  //TODO find out how to pass the data as binary to tauri without going through serde
  const buffer = await data.arrayBuffer();
  const array = [...new Uint8Array(buffer)];

  await invoke("process_message", { data: array });
}
const [socket, setSocket] = createSignal<WebSocket | undefined>();
createEffect<WebSocket | undefined>((previous) => {
  const id = identity();
  if (id === undefined) return;

  previous?.removeEventListener("message", handleMessage);
  previous?.close();

  const newSocket = new WebSocket(`ws://localhost:3000/${id}/messages`);
  newSocket.addEventListener("message", handleMessage);
  setSocket(newSocket);
  return newSocket;
}, socket());

type AppState = {
  identity: Resource<string>;
  setIdentity: Setter<string | undefined>;
  socket: Accessor<WebSocket | undefined>;
  groups: Resource<string[]>;
  setGroups: Setter<string[] | undefined>;
};

const state = {
  identity,
  setIdentity,
  socket,
  groups,
  setGroups,
} satisfies AppState;
const AppContext = createContext(state);

export function SocketProvider(properties: { children: JSX.Element }) {
  return (
    <AppContext.Provider value={state}>
      {properties.children}
    </AppContext.Provider>
  );
}

export function useWebSocket(onmessage?: (event: MessageEvent) => any) {
  const { socket } = useContext(AppContext);

  createEffect<WebSocket | undefined>((previous) => {
    const current = socket();
    if (onmessage === undefined || current === undefined) return current;

    previous?.removeEventListener("message", onmessage);
    current.addEventListener("message", onmessage);
    return current;
  }, socket());

  return (data: string | ArrayBufferLike | Blob | ArrayBufferView) =>
    socket()?.send(data);
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
