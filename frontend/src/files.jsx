import { useAgent } from "./agent";
import Upload from "./upload";
import { Link } from "solid-app-router";
import { createResource } from "solid-js";

export default function Files(props) {
  const agent = useAgent();

  const [files, { refetch }] = createResource(() => agent().File.list());

  return (
    <>
      <Link href="/">back</Link>
      <h1>Files</h1>
      <Upload callback={refetch} />
      <For each={files()}>
        {(pair, i) => <div>{pair[0]}</div>}
      </For>
    </>
  );
}
