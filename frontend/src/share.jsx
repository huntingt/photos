import { useAgent } from "./agent";
import { createResource, createSignal } from "solid-js";

export default function Share(props) {
  const agent = useAgent();
  const [sharers, { refetch }] = createResource(props.album, agent.Album.shared_with);
  
  const [searchString, setSearchString] = createSignal(null);
  const [search] = createResource(searchString, async email => {
    if (email) return await agent.User.emails(email);
    else return [];
  });

  const share = async (e) => {
    e.preventDefault();
    const el = e.target.elements;
    await agent.Album.share(
      props.album,
      el["email"].value,
      el["edit"].checked ? "Editor" : "Reader"
    );
    refetch();
  };

  return (
    <>
      <form on:submit={share} autocomplete="off">
        <input
          type="text"
          name="email"
          placeholder="Email"
          value={searchString()}
          onInput={e => setSearchString(e.target.value)}/>
        <label for="edit">Can edit:</label>
        <input type="checkbox" name="edit"/>
        <button type="submit">Share</button>
      </form>
      <For each={search()}>
        {(email, i) => <div onClick={() => setSearchString(email)}>{email}</div>}
      </For>
      <For each={sharers()}>
        {(user, i) => <div>{`"${user.email}": ${user.role}`}</div>}
      </For>
    </>
  );
}
