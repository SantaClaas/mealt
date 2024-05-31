/* @refresh reload */
import { render } from "solid-js/web";
import { Route, Router } from "@solidjs/router";

import "./styles.css";
import App from "./App";
import Groups from "./routes/Groups";
import { SocketProvider } from "./AppContext";

render(
  () => (
    <SocketProvider>
      <Router>
        <Route path="/" component={App} />
        <Route path="/groups/:id" component={Groups} />
      </Router>
    </SocketProvider>
  ),
  document.body
);
