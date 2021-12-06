import { useAgent } from "./agent";
import { createStore } from "solid-js/store";
import { Link, useNavigate } from "solid-app-router";

export default function Login(props) {
  const agent = useAgent();
  const navigate = useNavigate();

  const submit = async (e) => {
    e.preventDefault();
    const el = e.target.elements;
    await agent().User.create(
      el["email"].value,
      el["password"].value
    );
    navigate("/login");
  };

  return (
    <form on:submit={submit}>
      <input
        name="email"
        type="text"
        placeholder="Email"
        />
      <input
        name="password"
        type="password"
        placeholder="Password"
        />
      <button type="submit">Create</button>
      <Link href="/login">Login</Link>
    </form>
  );
}
