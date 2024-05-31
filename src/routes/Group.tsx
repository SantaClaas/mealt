import { useParams } from "@solidjs/router";
import { invoke } from "@tauri-apps/api/core";
import { For, createResource } from "solid-js";
import { useWebSocket } from "../AppContext";

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

  async function invitePackage(id: string) {
    if (!groupId()) return;

    const message = (await invoke("invite_package", {
      groupId: groupId(),
      packageId: id,
    })) as number[];

    console.debug("invite", { message });

    const data = Uint8Array.from(message);
    sendMessage(data);
  }
  return (
    <main>
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
    </main>
  );
}
