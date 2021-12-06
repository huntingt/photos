import { useAgent } from "./agent";
import { createSignal } from "solid-js";

export default function Upload(props) {
  const agent = useAgent();

  const [status, setStatus] = createSignal(null);

  const upload = async (e) => {
    e.preventDefault();
    const files = e.target.elements["files"].files;

    const uploads = [];
    const timestamps = {};
    for (const file of files) {
      if (file.type == "application/json") {
        try {
          let json = JSON.parse(await file.text());
          let timestamp = parseInt(json["creationTime"]["timestamp"], 10);
          timestamps[file.name.replace(".json", "")] = timestamp;
        } catch (err) {
          console.log(err);
        }
      } else {
        uploads.push(file);
      }
    }

    setStatus(() => [0, uploads.length]);
    let ids = [];
    for (const file of uploads) {
      try {
        ids.push(await agent().File.upload(file, timestamps[file.name]));
        setStatus(p => [p[0]+1, p[1]]);
      } catch (e) {
        setStatus(p => [p[0], p[1]-1]);
      }
    }
    if (props.album) {
      await agent().Album.add(props.album, ids);
    }
    if (props.callback) {
      props.callback();
    }
  }

  return (
    <form on:submit={upload}>
      <input type="file" name="files" multiple accept="video/*,image/*,application/JSON"/>
      <button type="submit">Upload</button>
      <Show when={status()}>
        {`${status()[0]}/${status()[1]}`}
      </Show>
    </form>
  );
}
