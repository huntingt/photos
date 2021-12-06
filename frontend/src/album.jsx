import { useParams, Link, useNavigate } from "solid-app-router";
import { useAgent } from "./agent";
import { createResource } from "solid-js";
import Share from "./share";
import Upload from "./upload";
import Viewer from "./viewer.jsx";

export default function Album(props) {
  const params = useParams();
  const agent = useAgent();
  const navigate = useNavigate();

  const [metadata, { refetch }] = createResource(params.id, agent().Album.metadata);

  const update = async (e) => {
    e.preventDefault();
    let el = e.target.elements;
    await agent().Album.update(
      params.id,
      el["name"].value,
      el["time_zone"].value
    );
    refetch();
  };

  const deleteAlbum = async (e) => {
    await agent().Album.delete(params.id);
    navigate("/");
  };

  return (
    <>
      <Link href="/">Back</Link>
      <Show when={metadata()}>
        <h1>{metadata().description.name}</h1>
        <Show when={metadata().role == "Owner"}>
          <button onClick={deleteAlbum}>Delete Album</button>
        </Show>
        <Show when={metadata().role != "Reader"}>
          <h3>Upload Files</h3>
          <Upload album={params.id} callback={refetch} />
          <h3>Settings</h3>
          {JSON.stringify(metadata())}
          <form on:submit={update}>
            <input
              type="text"
              value={metadata().description.name}
              name="name"/>
            <input
              type="text"
              value={metadata().description.time_zone}
              name="time_zone"/>
            <button type="submit">Update</button>
          </form>
        </Show>
        <h3>Share</h3>
        <Share album={params.id} />
        <Viewer album={params.id} metadata={metadata()} />
      </Show>
    </>
  );
}
