import { useAuth } from "./auth.jsx";

export default function Nested() {
  const [count, { increment, decrement }] = useAuth();
  return (
    <>
      <div>{count()}</div>
      <button onClick={increment}>+</button>
      <button onClick={decrement}>-</button>
    </>
  );
}
