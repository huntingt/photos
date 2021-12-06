import { render } from "solid-js/web";

import { Router, Routes, Route } from "solid-app-router";
import { Provider } from "./agent/index.jsx";
import Albums from "./albums.jsx";
import Login from "./login.jsx";
import Create from "./create.jsx";
import Album from "./album.jsx";
import Files from "./files.jsx";
import User from "./user.jsx";

render(() => (
  <Router>
    <Provider>
      <Routes>
        <Route path="/" element={<Albums />} />
        <Route path="/login" element={<Login />} />
        <Route path="/create" element={<Create />} />
        <Route path="/album/:id" element={<Album />} />
        <Route path="/files" element={<Files />} />
        <Route path="/user" element={<User />} />
      </Routes>
    </Provider>
  </Router>
), document.getElementById("root"));
