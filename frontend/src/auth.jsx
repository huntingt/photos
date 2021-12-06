import { createSignal, createContext, useContext } from "solid-js";

const AuthContext = createContext();

export function AuthProvider(props) {
  const [auth, setAuth] = createSignal(0);
  const store = [
    auth,
    {
      increment() {
        setAuth(c => c + 1);
      },
      decrement() {
        setAuth(c => c - 1);
      }
    }
  ];

  return (
    <AuthContext.Provider value={store}>
      {props.children}
    </AuthContext.Provider>
  );
}

export function useAuth() {
  return useContext(AuthContext);
}
