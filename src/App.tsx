import { invoke } from "@tauri-apps/api/core";
import { For, Show, createResource, createSignal } from "solid-js";

async function createUser(name: string) {
  await invoke("create_user", { name });
}

const isAuthenticated = async () =>
  (await invoke("is_authenticated")) as boolean;

const createGroup = async () => (await invoke("create_group")) as string;

function App() {
  const [isAuthenticatedResource, { refetch: refetchIsAuthenticated }] =
    createResource(isAuthenticated);

  const [groups, { mutate: mutateGroups }] = createResource(
    async () => (await invoke("get_groups")) as string[]
  );

  function handleSubmit(event: SubmitEvent) {
    event.preventDefault();
    // @ts-ignore
    const name = event.target.name.value;
    createUser(name);
    refetchIsAuthenticated();
  }

  async function handleCreateGroup() {
    const id = await createGroup();

    mutateGroups((groups) => (groups === undefined ? [id] : [...groups, id]));
  }

  return (
    <main>
      <Show when={!isAuthenticatedResource()}>
        <form onSubmit={handleSubmit}>
          <label for="name">Name</label>
          <input type="text" id="name" />
          <button type="submit">Submit</button>
        </form>
      </Show>

      <Show when={isAuthenticatedResource()}>
        <button onMouseDown={handleCreateGroup}>Create Group</button>
        <ol>
          <For each={groups()}>
            {(id) => (
              <li>
                <a href={`/groups/${id}`}>Group {id}</a>
              </li>
            )}
          </For>
        </ol>
      </Show>
    </main>
  );
}

export default App;