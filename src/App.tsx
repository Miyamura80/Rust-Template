import "./App.css";
import { Chat } from "./components/Chat";
import { UpdateNotification } from "./components/UpdateNotification";

function App() {
	return (
		<>
			<UpdateNotification />
			<Chat />
		</>
	);
}

export default App;
