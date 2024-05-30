import { useParams } from "@solidjs/router";

export default function Groups() {
  const parameters = useParams();
  return (
    <main>
      <h1>Group {parameters.id}</h1>
    </main>
  );
}
