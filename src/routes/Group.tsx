import { useParams } from "@solidjs/router";
import { invoke } from "@tauri-apps/api/core";
import { For, createResource } from "solid-js";
import { useAppState, useWebSocket } from "../AppContext";

async function getPackagesIndex() {
  const response = await fetch(`http://localhost:3000/packages`);

  if (!response.ok) throw new Error("Could not fetch packages");

  return (await response.json()) as string[];
}

export default function Group() {
  const parameters = useParams();

  const sendMessage = useWebSocket();
  const groupId = () => parameters.id;

  const [packages] = createResource(getPackagesIndex);
  const { identity, messages } = useAppState();

  async function invitePackage(id: string) {
    if (!groupId()) return;

    const message = (await invoke("invite_package", {
      groupId: groupId(),
      packageId: id,
    })) as number[];

    const data = Uint8Array.from(message);
    sendMessage(data);
  }

  async function handleMessageSubmit(event: SubmitEvent) {
    event.preventDefault();

    // @ts-ignore
    const message = event.target.message.value;
    (event.target as HTMLFormElement).reset();

    const data = (await invoke("create_message", {
      groupId: groupId(),
      message,
    })) as number[];

    const buffer = Uint8Array.from(data);

    sendMessage(buffer);
  }

  return (
    <main>
      <p>Your identity is {identity()}</p>
      <h1>Group {groupId()}</h1>
      <h2>Packages to invite</h2>
      <ol>
        <For each={packages()}>
          {(id) => (
            <li>
              <button onMouseDown={() => invitePackage(id)}>
                Package: {id}
              </button>
            </li>
          )}
        </For>
      </ol>

      <h2>Messages</h2>

      <form onSubmit={handleMessageSubmit}>
        <label for="message">Message</label>
        <input type="text" name="message" id="message" />
        <button type="submit">Send</button>
      </form>

      <For each={messages[groupId()]}>{(message) => <p>{message}</p>}</For>
    </main>
  );
}
