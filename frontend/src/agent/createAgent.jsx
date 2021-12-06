import { useNavigate } from "solid-app-router";
import { createSignal } from "solid-js";

/*
 * Adapted from the solid example code
 */
const API_ROOT = "/api";
const BASE_URL = window.location.protocol + "//" + window.location.host;

export default function createAgent() {
  const navigate = useNavigate();
  const [key, setKey] = createSignal(localStorage.getItem("api_key"));
  
  function logout() {
    setKey(null);
    localStorage.removeItem("api_key");

    navigate("/login", { replace: true });
  }

  async function send(method, route, data=null, resKey=null, query={}) {
    const headers = {};
    const opts = { method, headers };

    if (data) {
      headers["Content-Type"] = "application/json";
      opts.body = JSON.stringify(data);
    }

    const url = new URL(API_ROOT + route, BASE_URL);
    if (key()) {
      query.key = key();
    }

    for (const [k, v] of Object.entries(query)) {
      url.searchParams.append(k, v);
    }

    const response = await fetch(url, opts);

    if (response.status === 401) {
      logout()
    } else if (!response.ok) {
      throw response;
    }

    if (response.headers.get("Content-Type") == "application/json") {
      const json = await response.json();
      return resKey ? json[resKey] : json;
    }
  }

  const User = {
    create: (email, password) => send("POST", "/user", { email, password }),
    async emails(prefix=null, take=null, skip=null) {
      let query = {};
      if (prefix) query.prefix = prefix;
      if (skip)   query.skip = skip;
      if (take)   query.take = take;
      return await send("GET", "/user/emails", null, null, query);
    },
    async delete() {
      await send("DELETE", "/user");
      logout();
    },
    async login(email, password) {
      const key = await send("POST", "/user/auth", { email, password }, "key");
      setKey(key);
      localStorage.setItem("api_key", key);
    },
    async logout(prefix=null) {
      if (prefix == null) {
        prefix = key();
      }

      if (prefix == null) {
        return;
      }

      await send("DELETE", "/user/auth", { key: prefix });

      if (key()?.startsWith(prefix)) {
        logout();
      }
    },
    async change_password(old_password, new_password) {
      await send("PUT", "/user/auth", { old_password, new_password });
      logout();
    },
    sessions: () => send("GET", "/user/auth", null, "key_prefixes"),
  };

  const Album = {
    create: (name, time_zone) => send("POST", "/album", { name, time_zone }),
    update: (album_id, name, time_zone) => send("PATCH", `/album/${album_id}`, { name, time_zone }),
    delete: (album_id) => send("DELETE", `/album/${album_id}`),
    list: () => send("GET", "/album"),
    add: (album_id, ids) => send("POST", `/album/${album_id}/files`, { ids }),
    remove: (album_id, ids) => send("DELETE", `/album/${album_id}/files`, { ids }),
    share: (album_id, email, role) => send("POST", `/album/${album_id}/share`, { email, role }),
    shared_with: (album_id) => send("GET", `/album/${album_id}/share`),
    metadata: (album_id) => send("GET", `/album/${album_id}/serve/metadata`),
    fragment: (album_id, fid) => send("GET", `/album/${album_id}/serve/${fid}`),
  };

  const File = {
    delete: (file_id) => send("DELETE", `/file/${file_id}`),
    async list(prefix=undefined, skip=undefined, length=undefined) {
      return await send("POST", "/file/list", { prefix, skip, length }, "files");
    },
    resolveUrl(file_id, quality, album_id=null) {
      let route = API_ROOT + `/file/${quality}/${file_id}`;

      let query = [];
      if (key()) {
        query.push(`key=${key()}`);
      }
      if (album_id) {
        query.push(`album=${album_id}`);
      }
      if (query.length > 0) {
        route += "?" + query.reduce((a, b) => `${a}&${b}`);
      }

      return route;
    },
    async upload(file, timestamp=null) {
      const metadata = {
        name: file.name,
        last_modified: timestamp || Math.floor(file.lastModified / 1000),
        mime: file.type,
      };

      const enc_metadata = btoa(JSON.stringify(metadata))
        .replace('+', '-')
        .replace('/', '_')
        .replace(/=+$/, '');

      let route = API_ROOT + "/file";
      if (key()) {
        route += `?key=${key()}`;
      }

      const response = await fetch(route, {
        headers: {
          "upload-metadata": enc_metadata
        },
        method: "POST",
        body: file,
      });

      if (response.status === 401) {
        logout();
      } else if (!response.ok) {
        throw response;
      }

      let json = await response.json();
      return json["id"];
    }
  };

  return {
    User,
    Album,
    File
  };
}
