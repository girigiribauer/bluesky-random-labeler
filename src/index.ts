import dotenv from "dotenv";
import { LabelerServer } from "@skyware/labeler";
import { Bot, Profile } from "@skyware/bot";

dotenv.config();

const server = new LabelerServer({
  did: process.env.LABELER_DID ?? "",
  signingKey: process.env.SIGNING_KEY ?? "",
});

server.start(4000, (error) => {
  if (error) {
    console.error("Failed to start server:", error);
  } else {
    console.log("Labeler server running on port 4000");
  }
});

const bot = new Bot();
await bot.login({
  identifier: process.env.LABELER_DID ?? "",
  password: process.env.LABELER_PASSWORD ?? "",
});

// TODO: データベースをなくした際にフォローしてるけどラベルがない、みたいな状態になるのをなんとかする

const followHandler = ({ uri, user }: { user: Profile; uri: string }): void => {
  console.log(uri);
  user.labelAccount(["i-am-making-a-labeler"]);
};

bot.on("follow", followHandler);
bot.off("follow", followHandler);

// CLIじゃなく実行中のラベラーサーバーから追加できるかテスト
server.createLabel({
  uri: process.env.LABELER_DID ?? "",
  val: "cool-cat",
});
