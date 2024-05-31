/* @refresh reload */
import { render } from "solid-js/web";
import { Route, Router } from "@solidjs/router";

import "./styles.css";
import App from "./App";
import Group from "./routes/Group";
import { SocketProvider } from "./AppContext";

render(
  () => (
    <SocketProvider>
      <Router>
        <Route path="/" component={App} />
        <Route path="/groups/:id" component={Group} />
      </Router>
    </SocketProvider>
  ),
  document.body
);
