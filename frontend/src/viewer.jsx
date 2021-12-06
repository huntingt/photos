import { ViewerCore } from "./viewer.js";
import { createEffect, onCleanup } from "solid-js";
import { useAgent } from "./agent";

export default function Viewer(props) {
  let root;
  const agent = useAgent();

  createEffect(() => {
    const resolveUrl = (file_id, quality) =>
      agent.File.resolveUrl(file_id, quality, props.album);
    const fragment = (fid) =>
      agent.Album.fragment(props.album, fid);

    const viewer = new ViewerCore(
      root,
      resolveUrl,
      fragment,
      props.metadata.fragment_head
    );

    onCleanup(() => {
      viewer.uninstall();
    });
  });

  return <div ref={root} class="panel"></div>;
}
