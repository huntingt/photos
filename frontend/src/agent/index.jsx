import { createContext, useContext } from "solid-js";
import createAgent from "./createAgent.jsx";

const StoreContext = createContext();

export function Provider(props) {
  const agent = createAgent();

  return (
    <StoreContext.Provider value={agent}>
      {props.children}
    </StoreContext.Provider>
  );
}

export function useAgent() {
  return useContext(StoreContext);
}
