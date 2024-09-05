import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { SoluckGameProgram } from "../target/types/soluck_game_program";
import { PublicKey, SystemProgram } from "@solana/web3.js";
import { bs58 } from "@coral-xyz/anchor/dist/cjs/utils/bytes";

const LAMPORTS_PER_SOL = 1000000000;

describe("soluck-game-program", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env());

  const admin = anchor.web3.Keypair.generate();
  const adminSecondary = anchor.web3.Keypair.generate();

  const program = anchor.workspace
    .SoluckGameProgram as Program<SoluckGameProgram>;

  before("Init Config", async () => {
    const requestAirdrop = await program.provider.connection.requestAirdrop(
      admin.publicKey,
      LAMPORTS_PER_SOL * 100 // 100 SOL
    );
    const latestBlockHash =
      await program.provider.connection.getLatestBlockhash();
    await program.provider.connection.confirmTransaction({
      blockhash: latestBlockHash.blockhash,
      lastValidBlockHeight: latestBlockHash.lastValidBlockHeight,
      signature: requestAirdrop,
    });

    const tx = await program.methods
      .initConfig([admin.publicKey, adminSecondary.publicKey])
      .accounts({ signer: admin.publicKey })
      .signers([admin])
      .rpc();

    console.log("Your transaction signature", tx);
  });

  it("Init game!", async () => {
    const [configPDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("config")],
      program.programId
    );
    //fetch game count from configpda
    const configData = await program.account.configData.fetch(configPDA);
    const gameCount = configData.gameCount;

    const [gamePDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("game"), Buffer.from(gameCount.toString())],
      program.programId
    );

    // Add a new token to the whitelist
    const tx = await program.methods
      .initGame()
      .accounts({
        config: configPDA,
        game: gamePDA,
        auth: admin.publicKey,
      })
      .signers([admin])
      .rpc();

    console.log("Your transaction signature", tx);
  });

  it("It added a new token to whitelist!", async () => {
    const [configPDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("config")],
      program.programId
    );

    // Add a new token to the whitelist
    const tx = await program.methods
      .addTokenData(
        admin.publicKey, // Token address
        adminSecondary.publicKey // Oracle address
      )
      .accounts({
        config: configPDA,
        auth: admin.publicKey,
      })
      .signers([admin])
      .rpc();
  });

  it("Can enter game with SOL", async () => {
    const [configPDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("config")],
      program.programId
    );

    // Fetch game count from configPDA
    const configData = await program.account.configData.fetch(configPDA);
    const gameCount = configData.gameCount;

    const [gamePDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("game"), Buffer.from(gameCount.toString())],
      program.programId
    );
    console.log("gamePDA", gamePDA);
    console.log("configPDA", configPDA);

    // Derive the player PDA
    const [playerPDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("player"), admin.publicKey.toBuffer()],
      program.programId
    );
    console.log("playerPDA", playerPDA);

    // Add a new token to the whitelist
    let balance = await program.provider.connection.getBalance(gamePDA);
    console.log("balance balance", balance);
    const amount = new anchor.BN(15);
    const tx = await program.methods
      .enterGameSol(amount) // 0.001 SOL in lamports
      .accounts({
        config: configPDA,
        game: gamePDA,
        player: admin.publicKey,
        feed: "",
      })
      .signers([admin])
      .rpc();

    console.log("Transaction signature", tx);
    balance = await program.provider.connection.getBalance(gamePDA);
    console.log("balance balance", balance);
  });
});
