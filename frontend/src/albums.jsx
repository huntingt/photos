import { createResource, createEffect, For, Show } from "solid-js";
import { useAgent } from "./agent/index.jsx";
import { Link } from "solid-app-router";

function AlbumEntry(props) {
  function parseDate(unixTimeStamp) {
    const date = new Date(unixTimeStamp * 1000);
    return date.toLocaleDateString();
  }

  const format = () => {
    const d = props.details;
    let s = d.description.name;

    if (d.length == 1) {
      s += ` (1 photo)`;
    } else {
      s += ` (${d.length} photos)`;
    }

    if (d.date_range) {
      s += ` (${parseDate(d.date_range[0])} - ${parseDate(d.date_range[1])})`;
    }

    s += ` (${d.role})`;

    return s;
  };

  return (
    <div>
      <Link href={`/album/${props.id}`}>
        {format()}
      </Link>
    </div>
  );
}

export default function Albums(props) {
  const agent = useAgent();
  const [data, { refetch }] = createResource(agent().Album.list);

  const sortedAlbums = () => {
    let pairs = Object.entries(data());
    pairs.sort((a, b) => a[1].last_update < b[1].last_update);
    return pairs;
  };

  const newAlbum = async (e) => {
    e.preventDefault();
    const el = e.target.elements;
    await agent().Album.create(
      el["name"].value,
      Intl.DateTimeFormat().resolvedOptions().timeZone
    );
    el["name"].value = "";
    refetch();
  };

  return (
    <>
      <div><Link href="/user">User</Link></div>
      <div><Link href="/files">Files</Link></div>
      <h1>Albums</h1>
      <form on:submit={newAlbum}>
        <input
          name="name"
          type="text"
          placeholder="Name"
          required />
        <button type="submit">+ Album</button>
      </form>
      <Show when={data()}>
        <For each={sortedAlbums()}>
          {(pair, i) => <AlbumEntry id={pair[0]} details={pair[1]} />}
        </For>
      </Show>
    </>
  );
}
