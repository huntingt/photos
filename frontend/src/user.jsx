import { useAgent } from "./agent";
import { createResource } from "solid-js";
import { Link } from "solid-app-router";

export default function User(props) {
  const agent = useAgent();

  const [sessions, { refetch }] = createResource(agent().User.sessions);

  const change_password = async (e) => {
    e.preventDefault();
    let el = e.target.elements;
    await agent().User.change_password(
      el["old"].value,
      el["new"].value
    );
  };

  return (
    <>
      <Link href="/">Back</Link>
      <h1>User</h1>
      <h3>Change Password</h3>
      <form on:submit={change_password}>
        <input
          name="old"
          type="password"
          placeholder="Old password"/>
        <input
          name="new"
          type="password"
          placeholder="New password"/>
        <button type="submit">Update</button>
      </form>
      <h3>Sessions</h3>
      <button onClick={() => agent().User.logout()}>Logout</button>
      <For each={sessions()}>
        {(key, i) => (<div>
          {key}
          <button onClick={() => agent().User.logout(key).then(() => refetch())}>
            Logout
          </button>
        </div>)}
      </For>
    </>
  );
}
