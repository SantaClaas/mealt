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
  useContext,
} from "solid-js";
import { createStore } from "solid-js/store";

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

const [messages, setMessages] = createStore<Record<string, string[]>>({});

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

const state = {
  identity,
  setIdentity,
  socket,
  groups,
  setGroups,
  messages,
  setMessages,
};

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

function getGroupId(payload: unknown): string {
  if (
    typeof payload !== "object" ||
    payload === null ||
    !("group_id" in payload) ||
    typeof payload.group_id !== "string"
  )
    throw new Error("Unexpected join group event payload");

  return payload.group_id;
}

function getMessage(payload: unknown): string {
  if (
    typeof payload !== "object" ||
    payload === null ||
    !("message" in payload) ||
    typeof payload.message !== "string"
  )
    throw new Error("Unexpected new message event payload");

  return payload.message;
}

listen("join_group", (event) => {
  const group = getGroupId(event.payload);
  setGroups((groups) => (groups === undefined ? [group] : [...groups, group]));
});

listen("new_message", (event) => {
  const groupId = getGroupId(event.payload);
  const message = getMessage(event.payload);

  //TODO check if using an object (/record) has perfomance impact compared to a map
  setMessages(groupId, (messages) =>
    messages ? [...messages, message] : [message]
  );
});
